//! Import

use {
	super::ARCHIVE_VERSION,
	crate::{
		archive::{EntryKind, MAGIC},
		stats::Stats,
		util::{AppError, OsStrAsPath, OsStrRange, OsStrStrip, OsStringPath, var_int},
	},
	app_error::{Context, bail, ensure},
	core::mem,
	std::{
		ffi::OsString,
		fs,
		io::{self, prelude::Seek},
		os::unix::prelude::OsStringExt,
		path::Path,
	},
};

/// Importer
#[derive(derive_more::Debug)]
pub struct Importer {
	/// Input
	#[debug(skip)]
	input: io::BufReader<fs::File>,
}

impl Importer {
	/// Creates a new importer that imports from a file
	pub fn new(path: &Path) -> Result<Self, AppError> {
		let mut input = fs::File::open_buffered(path).context("Unable to open import file")?;
		self::read_archive_header(&mut input)?;

		Ok(Self { input })
	}

	/// Seeks this importer to a path within relative to an entry.
	///
	/// The path may be either relative or absolute (if the path matches the root path of this import)
	pub fn seek_path(
		&mut self,
		mut entry_import: DirEntry,
		mut entry_path: OsString,
		path: &Path,
	) -> Result<(DirEntry, OsString), AppError> {
		let path = match path.is_relative() {
			true => path,
			false => match path.strip_prefix(&entry_path) {
				Ok(path) => path,
				Err(_) => bail!(
					"Specified path is not within file tree.\n\tExpected a path within {}\n\tFound                  {}",
					entry_path.display(),
					path.display()
				),
			},
		};
		let path = path.trim_trailing_sep();

		let mut cmpts = path.components();
		let mut prev_path = Path::new("");
		let mut cur_name = OsString::new();
		while let Some(cmpt) = cmpts.next() {
			let cur_path = path
				.as_os_str()
				.strip_suffix(cmpts.as_path())
				.expect("Path should be a suffix");
			match cmpt {
				std::path::Component::Prefix(_) | std::path::Component::RootDir =>
					unreachable!("Found a prefix/root directory in a relative path: {cmpt:?}"),
				std::path::Component::CurDir => (),
				std::path::Component::ParentDir => bail!("`..` isn't supported on the paths to filter yet"),
				std::path::Component::Normal(name) => match entry_import.kind {
					DirEntryKind::Dir(dir) => 'find_entry: {
						for _ in 0..dir.entries_len {
							cur_name.clear();
							let (entry, _) = self.read_entry(&mut cur_name).with_context(|| {
								format!(
									"Unable to read entry at {}",
									entry_path.as_path().join(prev_path).display(),
								)
							})?;

							if cur_name != name {
								if let DirEntryKind::Dir(dir) = entry.kind {
									self.skip_dir_entries(dir)?;
									_ = self::read_dir_footer(&mut self.input)?;
								}

								continue;
							}

							entry_import = entry;
							entry_path.push_path(name);
							break 'find_entry;
						}

						bail!(
							"Unable to find file {} in {}",
							name.display(),
							entry_path.as_path().join(prev_path).display(),
						);
					},
					_ => bail!(
						"Expected a directory at {}",
						entry_path.as_path().join(prev_path).display()
					),
				},
			}
			prev_path = Path::new(cur_path);
		}

		Ok((entry_import, entry_path))
	}

	/// Seeks to an entry
	pub fn seek_entry(&mut self, entry: &DirEntry) -> Result<(), AppError> {
		self.input.seek(io::SeekFrom::Start(entry.import_pos))?;
		Ok(())
	}

	/// Skips over a directory's entries
	pub fn skip_dir_entries(&mut self, dir: DirHeader) -> Result<(), AppError> {
		let offset = i64::try_from(dir.entries_offset).context("Entries offset didn't fit into an `i64`")?;
		self.input.seek_relative(offset)?;
		Ok(())
	}

	/// Reads an entry.
	///
	/// Appends the entry name to `name` and returns the appended
	/// range of the name.
	pub fn read_entry(&mut self, name: &mut OsString) -> Result<(DirEntry, OsStrRange), AppError> {
		self::read_entry(&mut self.input, name)
	}

	/// Reads an entry's data, returning it's total stats
	pub fn read_entry_stats(&mut self, entry: DirEntry) -> Result<Stats, AppError> {
		match entry.kind {
			DirEntryKind::Dir(dir) => {
				self.skip_dir_entries(dir)?;
				let footer = self.read_dir_footer()?;

				Ok(footer.stats)
			},
			DirEntryKind::File(file) => Ok(Stats {
				files:  1,
				size:   file.size,
				blocks: file.blocks,
			}),
			DirEntryKind::Symlink(symlink) => Ok(Stats {
				files:  1,
				size:   symlink.size,
				blocks: symlink.blocks,
			}),
		}
	}

	/// Reads a directory footer
	pub fn read_dir_footer(&mut self) -> Result<DirFooter, AppError> {
		self::read_dir_footer(&mut self.input)
	}
}

#[derive(Clone, Copy, Debug)]
pub struct DirEntry {
	pub import_pos: u64,
	pub kind:       DirEntryKind,
}

impl DirEntry {
	/// Returns if this entry has any children
	pub fn has_children(&self) -> bool {
		self.kind
			.try_as_dir_ref()
			.is_some_and(|lhs_dir| lhs_dir.entries_len != 0)
	}
}

#[derive(Clone, Copy, Debug)]
#[derive(strum::EnumTryAs, strum::EnumIs)]
pub enum DirEntryKind {
	Dir(DirHeader),
	File(File),
	Symlink(Symlink),
}

#[derive(Clone, Copy, Debug)]
pub struct DirHeader {
	pub entries_offset: u64,
	pub entries_len:    u64,
}

#[derive(Clone, Copy, Debug)]
pub struct DirFooter {
	pub stats: Stats,
}

#[derive(Clone, Copy, Debug)]
pub struct File {
	pub size:   u64,
	pub blocks: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct Symlink {
	pub size:   u64,
	pub blocks: u64,
}

/// Reads the archive header
fn read_archive_header<R: io::Read>(reader: &mut R) -> Result<(), AppError> {
	let magic = reader.read_array().context("Expected magic")?;
	ensure!(magic == MAGIC, "Found wrong magic {magic:?}, expected {MAGIC:?}");

	let version = reader.read_le::<u32>()?;
	ensure!(
		version == ARCHIVE_VERSION,
		"Found wrong version {version:?}, expected {ARCHIVE_VERSION:?}"
	);

	Ok(())
}

/// Reads an entry
fn read_entry<R: io::BufRead + io::Seek>(
	reader: &mut R,
	name: &mut OsString,
) -> Result<(DirEntry, OsStrRange), AppError> {
	let name_range = self::read_os_str(reader, name).context("Unable to read entry name")?;
	let kind = self::read_u8(reader).context("Unable to read entry kind")?;
	let kind = EntryKind::from_repr(kind).with_context(|| format!("Unknown file kind {kind:#04x}"))?;
	let kind = match kind {
		EntryKind::Dir => {
			let dir = self::read_dir_header(reader).context("Unable to read entry directory header")?;
			DirEntryKind::Dir(dir)
		},
		EntryKind::File => {
			let file = self::read_file(reader).context("Unable to read file")?;
			DirEntryKind::File(file)
		},
		EntryKind::Symlink => {
			let symlink = self::read_symlink(reader).context("Unable to read symlink")?;
			DirEntryKind::Symlink(symlink)
		},
	};

	let import_pos = reader.stream_position().context("Unable to get import position")?;

	Ok((DirEntry { import_pos, kind }, name_range))
}

/// Reads a directory header
fn read_dir_header<R: io::Read>(reader: &mut R) -> Result<DirHeader, AppError> {
	let entries_offset = self::read_u64_fixed(reader)?;
	let entries_len = self::read_u64_fixed(reader)?;

	Ok(DirHeader {
		entries_offset,
		entries_len,
	})
}

/// Reads a directory footer
fn read_dir_footer<R: io::Read>(reader: &mut R) -> Result<DirFooter, AppError> {
	let size = self::read_u64(reader)?;
	let blocks = self::read_u64(reader)?;
	let files = self::read_u64(reader)?;

	let stats = Stats { files, size, blocks };
	Ok(DirFooter { stats })
}

/// Reads a file
fn read_file<R: io::Read>(reader: &mut R) -> Result<File, AppError> {
	let size = self::read_u64(reader)?;
	let blocks = self::read_u64(reader)?;

	Ok(File { size, blocks })
}

/// Reads a symlink
fn read_symlink<R: io::Read>(reader: &mut R) -> Result<Symlink, AppError> {
	let size = self::read_u64(reader)?;
	let blocks = self::read_u64(reader)?;

	Ok(Symlink { size, blocks })
}

/// Reads a variable-length `u64`
fn read_u64<R: io::Read>(reader: &mut R) -> Result<u64, AppError> {
	let n = var_int::decode(reader)?;
	Ok(n)
}

/// Reads a fixed-length `u64`
fn read_u64_fixed<R: io::Read>(reader: &mut R) -> Result<u64, AppError> {
	let bytes = reader.read_array()?;
	let n = u64::from_le_bytes(bytes);
	Ok(n)
}

/// Reads a `u8`
fn read_u8<R: io::Read>(reader: &mut R) -> Result<u8, AppError> {
	let n = reader.read_array().context("Expected u8")?;
	let n = u8::from_le_bytes(n);
	Ok(n)
}

/// Reads an `OsStr`
fn read_os_str<R: io::BufRead>(reader: &mut R, s: &mut OsString) -> Result<OsStrRange, AppError> {
	fn inner<R: io::BufRead>(reader: &mut R, bytes: &mut Vec<u8>) -> Result<(), AppError> {
		loop {
			let reader_bytes = reader.fill_buf()?;
			ensure!(!reader_bytes.is_empty(), "EOF while reading string");
			match memchr::memchr(b'\0', reader_bytes) {
				Some(idx) => {
					bytes.extend_from_slice(&reader_bytes[..idx]);
					reader.consume(idx + 1);
					break Ok(());
				},
				None => {
					bytes.extend_from_slice(reader_bytes);
					let len = reader_bytes.len();
					reader.consume(len);
				},
			}
		}
	}

	let start_idx = s.len();
	let mut bytes = mem::take(s).into_vec();
	let res = inner(reader, &mut bytes);
	*s = OsString::from_vec(bytes);
	let end_idx = s.len();

	res?;
	Ok((start_idx..end_idx).into())
}
