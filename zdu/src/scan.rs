//! Scan

use {
	crate::{
		archive::export::{DirEntryExporter, ExportError, Exporter},
		stats::Stats,
		util::AppError,
	},
	std::{
		collections::{HashMap, HashSet},
		ffi::OsStr,
		fs,
		io,
		os::unix::{fs::DirEntryExt2, prelude::MetadataExt as _},
		path::{Path, PathBuf},
	},
};

/// Scans a directory
pub fn dir(path: &Path, exporter: &mut Exporter, entry: DirEntryExporter, args: &mut Args) -> Result<Stats, ScanError> {
	let entries = path
		.read_dir()
		.map_err(|err| ScanError::ReadDir {
			path:  path.to_path_buf(),
			inner: err,
		})?
		.filter_map(|res| {
			res.inspect_err(|err| tracing::warn!("Skipping unreadable directory entry in {path:?}: {err:?}"))
				.ok()
		})
		.collect::<Vec<_>>();

	let dir = entry.write_dir(exporter)?;
	let mut entries_stats = Stats::default();
	let mut written_entries = 0;
	for entry in &entries {
		args.depth += 1;

		match self::entry(entry, exporter, args) {
			Ok(stats) =>
				if let Some(stats) = stats {
					entries_stats += stats;
					written_entries += 1;
				},

			// Note: `entry` only writes the entry if it succeeds, so we can just ignore it here.
			Err(err) => tracing::warn!(
				"Unable to scan directory entry {}: {:?}",
				entry.path().display(),
				AppError::from(err)
			),
		}

		args.depth -= 1;
	}
	dir.finish(exporter, written_entries, entries_stats)?;

	Ok(entries_stats)
}

/// Scans a directory entry.
///
/// Returns `Ok(Some(stats))` if the entry was written
fn entry(entry: &fs::DirEntry, exporter: &mut Exporter, args: &mut Args) -> Result<Option<Stats>, ScanError> {
	// Note: Despite `scan_from_metadata` also performing this check, it's
	//       faster if we also do it here to avoid reading the file's metadata.
	let path = entry.path();
	if args.ignore.contains(&path) {
		tracing::debug!("Ignoring file in ignore list: {path:?}");
		return Ok(None);
	}

	// Note: On failing reading metadata, we haven't written anything yet, so
	//       there's nothing to fix up on the exporter.
	let metadata = match args.follow_symlinks {
		true => fs::metadata(entry.path()),
		false => entry.metadata(),
	}
	.map_err(|err| ScanError::ReadMetadata {
		path:  path.clone(),
		inner: err,
	})?;

	let stats = self::entry_from_metadata(&path, entry.file_name_ref(), &metadata, exporter, args)?;
	Ok(stats)
}

/// Scans an entry from it's metadata
///
/// Returns `Ok(Some(stats))` if the entry was written
/// with the (recursive) entry stats
pub fn entry_from_metadata(
	path: &Path,
	name: &OsStr,
	metadata: &fs::Metadata,
	exporter: &mut Exporter,
	args: &mut Args,
) -> Result<Option<Stats>, ScanError> {
	if args.ignore.contains(path) {
		tracing::debug!("Ignoring file in ignore list: {path:?}");
		return Ok(None);
	}

	// Note: If we're following symlinks, then this will detect ones that
	//       escape the current file system.
	if let Some(root_dev) = args.root_dev &&
		metadata.dev() != root_dev
	{
		tracing::debug!("Ignoring file in a different filesystem: {path:?}");
		return Ok(None);
	}

	// Note: This check must be done *after* checking the filesystem device, since otherwise
	//       we might consider duplicates that aren't actually duplicates
	// TODO: When not following symlinks, we still insert directories into the hardlink inodes seen,
	//       but maybe we shouldn't?
	if metadata.nlink() > 1 &&
		!args
			.hardlink_inodes_seen
			.entry(metadata.dev())
			.or_default()
			.insert(metadata.ino())
	{
		match metadata.is_dir() {
			// Note: This can only happen if we're following symlinks and we encounter a symlink look
			true => tracing::warn!("Ignoring symlink directory loop: {path:?}"),
			false => tracing::debug!("Ignoring hardlink: {path:?}"),
		}
		return Ok(None);
	}

	let mut stats = Stats {
		files:  1,
		size:   metadata.size(),
		blocks: metadata.blocks(),
	};
	match metadata.file_type() {
		file_type if file_type.is_dir() => {
			let entry = exporter.write_entry_from_metadata(name, metadata)?;
			// Note: `dir` only fails if it can't read the directory, in which case we undo the previous write
			match self::dir(path, exporter, entry, args) {
				Ok(entries_stats) => stats += entries_stats,
				Err(err) => {
					exporter.undo_last_entry()?;
					return Err(ScanError::Recursive {
						path:  path.to_path_buf(),
						inner: Box::new(err),
					});
				},
			}
		},
		file_type if file_type.is_file() => exporter
			.write_entry_from_metadata(name, metadata)?
			.write_file(exporter)?,
		file_type if file_type.is_symlink() => exporter
			.write_entry_from_metadata(name, metadata)?
			.write_symlink(exporter)?,
		_ => {
			tracing::warn!("Ignoring unknown file type of {}", path.display());
			return Ok(None);
		},
	}

	Ok(Some(stats))
}

/// Scan arguments
#[derive(Debug)]
pub struct Args {
	/// Root device, if we should filter by filesystem
	pub root_dev: Option<u64>,

	/// Follow symlinks
	pub follow_symlinks: bool,

	/// Directories to ignore.
	///
	/// They are all made absolute
	pub ignore: HashSet<PathBuf>,

	/// Inodes with hardlinks seen by device+inode
	pub hardlink_inodes_seen: HashMap<u64, HashSet<u64>>,

	/// Current depth
	pub depth: usize,
}

/// Scan error
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
	#[error("Unable to read directory {}", path.display())]
	ReadDir {
		path:  PathBuf,
		#[source]
		inner: io::Error,
	},

	#[error("Unable to read metadata of {}", path.display())]
	ReadMetadata {
		path:  PathBuf,
		#[source]
		inner: io::Error,
	},

	#[error("While reading path {}", path.display())]
	Recursive {
		path:  PathBuf,
		#[source]
		inner: Box<Self>,
	},

	#[error(transparent)]
	Export(#[from] ExportError),
}
