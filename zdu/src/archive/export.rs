//! Export

use {
	super::{ARCHIVE_VERSION, DIR_HEADER_SIZE, MAGIC},
	crate::{archive::EntryKind, stats::Stats, util::var_int},
	std::{
		ffi::OsStr,
		fs,
		io::{
			self,
			prelude::{Seek, Write},
		},
		os::unix::prelude::{MetadataExt, OsStrExt},
		path::{Path, PathBuf},
	},
};

/// Exporter
#[derive(derive_more::Debug)]
pub struct Exporter {
	/// Output file
	#[debug(skip)]
	output: io::BufWriter<fs::File>,

	/// Last entry's position
	last_entry_pos: u64,
}

impl Exporter {
	/// Creates a new exporter that exports to a file.
	///
	/// Writes the header already
	pub fn new(path: &Path) -> Result<Self, ExportError> {
		let mut output = fs::File::create_buffered(path).map_err(|err| ExportError::CreateOutput {
			path:  path.to_path_buf(),
			inner: err,
		})?;

		self::write_archive_header(&mut output)?;

		Ok(Self {
			output,
			last_entry_pos: 0,
		})
	}

	/// Finishes writing the export
	pub fn finish(mut self) -> Result<(), ExportError> {
		self.output.flush()?;
		Ok(())
	}

	/// Writes an entry from it's metadata
	pub fn write_entry_from_metadata<'metadata>(
		&mut self,
		name: &OsStr,
		metadata: &'metadata fs::Metadata,
	) -> Result<DirEntryExporter<'metadata>, ExportError> {
		// TODO: Calling `stream_position` on a `fs::File` always does a syscall,
		//       we should do something about that.
		self.last_entry_pos = self.output.stream_position()?;
		self::write_entry_header(&mut self.output, name)?;
		Ok(DirEntryExporter { metadata })
	}

	/// Undoes the previously written entry
	pub fn undo_last_entry(&mut self) -> Result<(), ExportError> {
		self.output.seek(io::SeekFrom::Start(self.last_entry_pos))?;
		Ok(())
	}
}

/// Directory entry exporter
// TODO: Guard against forgetting to write `DirEntryExporter::finish`
pub struct DirEntryExporter<'a> {
	metadata: &'a fs::Metadata,
}

impl<'a> DirEntryExporter<'a> {
	/// Starts writing a directory
	pub fn write_dir(self, exporter: &mut Exporter) -> Result<DirExporter<'a>, ExportError> {
		self::write_u8(&mut exporter.output, EntryKind::Dir as u8)?;

		// Skip over the header
		// Note: Seeking a `BufWriter` always flushes out the buffer, but we need
		//       to get the current position anyway and `BufWriter` doesn't expose
		//       any way other than `seek(0)`, so we might as well skip past the
		//       buffer we're about to write.
		let header_pos = exporter
			.output
			.seek(io::SeekFrom::Current(i64::from(DIR_HEADER_SIZE)))? -
			u64::from(DIR_HEADER_SIZE);

		Ok(DirExporter {
			metadata: self.metadata,
			header_pos,
		})
	}

	/// Writes a file
	pub fn write_file(self, exporter: &mut Exporter) -> Result<(), ExportError> {
		self::write_u8(&mut exporter.output, EntryKind::File as u8)?;
		self::write_file(&mut exporter.output, self.metadata.size(), self.metadata.blocks())?;
		Ok(())
	}

	/// Writes a symlink
	pub fn write_symlink(self, exporter: &mut Exporter) -> Result<(), ExportError> {
		self::write_u8(&mut exporter.output, EntryKind::Symlink as u8)?;
		self::write_symlink(&mut exporter.output, self.metadata.size(), self.metadata.blocks())?;
		Ok(())
	}
}


/// Directory exporter
// TODO: Guard against forgetting to write `DirExporter::finish`
pub struct DirExporter<'a> {
	metadata: &'a fs::Metadata,

	/// Header position
	header_pos: u64,
}

impl DirExporter<'_> {
	/// Finishes writing this directory
	pub fn finish(self, exporter: &mut Exporter, len: u64, children_stats: Stats) -> Result<(), ExportError> {
		let entries_end_pos = exporter.output.stream_position()?;

		let entry_stats = Stats {
			files:  1,
			size:   self.metadata.size(),
			blocks: self.metadata.blocks(),
		};
		let total_stats = entry_stats + children_stats;
		self::write_dir_footer(&mut exporter.output, total_stats)?;
		let entry_end_pos = exporter.output.stream_position()?;

		let entries_start_pos = self.header_pos + u64::from(DIR_HEADER_SIZE);
		let entries_offset = entries_end_pos - entries_start_pos;

		exporter.output.seek(io::SeekFrom::Start(self.header_pos))?;
		self::write_dir_header(&mut exporter.output, entries_offset, len)?;
		exporter.output.seek(io::SeekFrom::Start(entry_end_pos))?;

		Ok(())
	}
}

/// Writes the archive header
fn write_archive_header<W: io::Write>(writer: &mut W) -> Result<(), ExportError> {
	writer.write_all(&MAGIC)?;
	writer.write_all(&ARCHIVE_VERSION.to_le_bytes())?;

	Ok(())
}

/// Writes an entry header
fn write_entry_header<W: io::Write>(writer: &mut W, name: &OsStr) -> Result<(), ExportError> {
	self::write_os_str(writer, name)?;

	Ok(())
}

/// Writes a directory header
fn write_dir_header<W: io::Write>(writer: &mut W, entries_offset: u64, entries_len: u64) -> Result<(), ExportError> {
	self::write_u64_fixed(writer, entries_offset)?;
	self::write_u64_fixed(writer, entries_len)?;
	Ok(())
}

/// Writes a directory footer
fn write_dir_footer<W: io::Write>(writer: &mut W, total_stats: Stats) -> Result<(), ExportError> {
	self::write_u64(writer, total_stats.size)?;
	self::write_u64(writer, total_stats.blocks)?;
	self::write_u64(writer, total_stats.files)?;
	Ok(())
}

/// Writes a file
fn write_file<W: io::Write>(writer: &mut W, size: u64, blocks: u64) -> Result<(), ExportError> {
	self::write_u64(writer, size)?;
	self::write_u64(writer, blocks)?;
	Ok(())
}

/// Writes a symlink
fn write_symlink<W: io::Write>(writer: &mut W, size: u64, blocks: u64) -> Result<(), ExportError> {
	self::write_u64(writer, size)?;
	self::write_u64(writer, blocks)?;
	Ok(())
}

/// Writes a variable-length `u64`
fn write_u64<W: io::Write>(writer: &mut W, n: u64) -> Result<(), ExportError> {
	var_int::encode(writer, n)?;
	Ok(())
}

/// Writes a fixed-length `u64`
fn write_u64_fixed<W: io::Write>(writer: &mut W, n: u64) -> Result<(), ExportError> {
	writer.write_all(&n.to_le_bytes())?;
	Ok(())
}

/// Writes a `u8`
fn write_u8<W: io::Write>(writer: &mut W, n: u8) -> Result<(), ExportError> {
	writer.write_all(&n.to_le_bytes())?;
	Ok(())
}

/// Writes an `OsStr`
fn write_os_str<W: io::Write>(writer: &mut W, s: &OsStr) -> Result<(), ExportError> {
	writer.write_all(s.as_bytes())?;
	writer.write_all(&[0])?;
	Ok(())
}

/// Export error
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
	// TODO: Split this up into WriteOutput and other IO errors?
	#[error(transparent)]
	Io(#[from] io::Error),

	#[error("Unable to create export file {}", path.display())]
	CreateOutput {
		path:  PathBuf,
		#[source]
		inner: io::Error,
	},
}
