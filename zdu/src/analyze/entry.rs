//! Analyze entry

use {
	crate::{
		archive::import::{self, Importer},
		stats::Stats,
		util::{AppError, OsStrRange},
	},
	app_error::Context,
	core::fmt::Debug,
	std::ffi::OsString,
};

/// Config
#[derive(Debug)]
pub struct Config<'a> {
	pub importer: &'a mut Importer,

	pub names: &'a mut OsString,
}

/// Reads an entry from an import
pub fn read_from_import(config: Config) -> Result<ImportEntry, AppError> {
	let (import, name_range) = config
		.importer
		.read_entry(config.names)
		.context("Unable to read entry")?;

	let stats = config.importer.read_entry_stats(import)?;

	Ok(ImportEntry {
		name_range,
		stats,
		import,
	})
}

/// Directory entry.
#[derive(Clone, Copy, Debug)]
#[derive(strum::EnumIs)]
pub enum Entry {
	/// A directory entry from the import
	Import(ImportEntry),

	/// A group of directory entries
	Group(GroupEntry),
}

/// Import entry
#[derive(Clone, Copy, Debug)]
pub struct ImportEntry {
	pub name_range: OsStrRange,
	pub stats:      Stats,
	pub import:     import::DirEntry,
}


/// Group of entries
#[derive(Clone, Copy, Debug)]
pub struct GroupEntry {
	/// Number of entries in this group
	pub len: u64,

	/// Total stats
	pub stats: Stats,
}

impl Entry {
	/// Gets the stats of this entry
	pub fn stats(&self) -> Stats {
		match self {
			Self::Import(entry) => entry.stats,
			Self::Group(entry) => entry.stats,
		}
	}

	/// Returns if this entry is a group or a directory
	pub fn is_group_or_dir(&self) -> bool {
		match self {
			Self::Import(entry) => entry.import.kind.is_dir(),
			Self::Group(_) => true,
		}
	}
}
