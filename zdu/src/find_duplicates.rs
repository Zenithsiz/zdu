//! Finds duplicates in an archive

use {
	crate::{
		archive::import::{self, Importer},
		util::{self, AppError, OsStrAsPath},
	},
	std::{
		collections::HashMap,
		ffi::OsString,
		path::{Path, PathBuf},
	},
};


#[derive(Debug)]
pub struct Config<'a> {
	pub importer: &'a mut Importer,

	pub entry_import: import::DirEntry,
	pub entry_path:   OsString,
}

#[derive(Debug)]
pub struct Output {
	pub files_by_size: HashMap<u64, FilesBySize>,
	pub files_total:   usize,
	pub files_read:    usize,
	pub size_total:    u64,
	pub size_read:     u64,
}

/// Finds duplicated files within an archive import
pub fn find_duplicates(config: Config<'_>) -> Result<Output, AppError> {
	let mut files_by_size = HashMap::new();
	let mut args = Args {
		depth:         0,
		files_by_size: &mut files_by_size,
		cur_path:      config.entry_path.as_path().to_path_buf(),
		cur_name:      config.entry_path,
		files_total:   0,
		files_read:    0,
		size_total:    0,
		size_read:     0,
	};
	self::entry(config.importer, &config.entry_import, &mut args)?;

	Ok(Output {
		files_total: args.files_total,
		files_read: args.files_read,
		size_total: args.size_total,
		size_read: args.size_read,
		files_by_size,
	})
}

fn entry(importer: &mut Importer, entry: &import::DirEntry, args: &mut Args) -> Result<(), AppError> {
	match entry.kind {
		import::DirEntryKind::Dir(dir) => {
			for _ in 0..dir.entries_len {
				args.cur_name.clear();
				let (entry, _) = importer.read_entry(&mut args.cur_name)?;

				args.depth += 1;
				args.cur_path.push(args.cur_name.as_path());
				self::entry(importer, &entry, args)?;
				args.cur_path.pop();
				args.depth -= 1;
			}

			_ = importer.read_dir_footer()?;
		},

		// Special-case empty files to avoid opening them
		import::DirEntryKind::File(import::File { size: 0, .. }) => {
			args.files_total += 1;
			args.files_by_size
				.entry(0)
				.or_default()
				.files_by_md5
				.entry(util::empty_md5())
				.or_default()
				.push(args.cur_path.as_path().into());
		},

		import::DirEntryKind::File(import::File { size, .. }) => {
			args.files_total += 1;
			args.size_total += size;

			let files_by_size = args.files_by_size.entry(size).or_default();

			// If we have an unread path, resolve it right now.
			if let Some(unread_path) = files_by_size.last_unread.take() {
				match util::file_md5(&unread_path) {
					Ok(md5) => files_by_size.files_by_md5.entry(md5).or_default().push(unread_path),
					Err(err) => tracing::warn!(
						"Unable to compute md5 of {}, discarding it: {err:?}",
						unread_path.display()
					),
				}
				args.files_read += 1;
				args.size_read += size;
			}

			// Then if we have no files computed yet, we can leave it for later.
			if files_by_size.files_by_md5.is_empty() {
				assert!(files_by_size.last_unread.is_none());
				files_by_size.last_unread = Some(args.cur_path.as_path().into());
				return Ok(());
			}

			// Otherwise, we need to compute the md5 and add it to the list
			let md5 = match util::file_md5(&args.cur_path) {
				Ok(md5) => md5,
				Err(err) => {
					tracing::warn!("Unable to compute md5 of {}: {err:?}", args.cur_path.display());
					return Ok(());
				},
			};
			args.files_read += 1;
			args.size_read += size;

			files_by_size
				.files_by_md5
				.entry(md5)
				.or_default()
				.push(args.cur_path.as_path().into());
		},

		import::DirEntryKind::Symlink(_) => tracing::info!("Skipping symlink: {}", args.cur_path.display()),
	}

	Ok(())
}

#[derive(Debug)]
struct Args<'a> {
	depth:         usize,
	files_by_size: &'a mut HashMap<u64, FilesBySize>,

	/// Current entry's path
	cur_path: PathBuf,

	/// Current entry's path
	cur_name: OsString,

	files_total: usize,
	files_read:  usize,

	size_total: u64,
	size_read:  u64,
}

#[derive(Default, Debug)]
pub struct FilesBySize {
	/// Last file with this size that hasn't been read yet.
	// Note: Only `Some(...)` if `files_by_md5` is empty, since otherwise
	//       we'd resolve the file immediately.
	pub last_unread: Option<Box<Path>>,

	/// Files by their md5 hash
	pub files_by_md5: HashMap<md5::Digest, Vec<Box<Path>>>,
}
