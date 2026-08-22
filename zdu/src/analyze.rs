//! Analyzes an import

pub mod entry;
pub mod sort;

use {
	self::{
		entry::{Entry, GroupEntry, ImportEntry},
		sort::SortOrder,
	},
	crate::{
		BLOCK_SIZE,
		archive::import::{self, Importer},
		stats::Stats,
		tree_display::TreeDisplay,
		util::{self, AppError, Colorize},
	},
	app_error::Context,
	core::range,
	std::{ffi::OsString, io},
};

pub struct Config<'a, W> {
	pub output:   &'a mut W,
	pub importer: &'a mut Importer,

	pub entry:      import::DirEntry,
	pub entry_name: OsString,

	pub max_depth:         Option<usize>,
	pub max_files_per_dir: Option<usize>,
	pub sort_order:        SortOrder,
}

/// Analyzes an import
pub fn analyze<W: io::Write>(config: Config<'_, W>) -> Result<(), AppError> {
	let mut names = config.entry_name;
	let mut entries = Vec::with_capacity(1);

	let name_range = (0..names.len()).into();
	entries.push(Entry::Import(ImportEntry {
		name_range,
		stats: config.importer.read_entry_stats(config.entry)?,
		import: config.entry,
	}));
	let entry_idx = 0;
	config.importer.seek_entry(&config.entry)?;

	let mut args = Args {
		output:       config.output,
		importer:     config.importer,
		tree_display: TreeDisplay::new(),

		names:   &mut names,
		entries: &mut entries,

		max_depth:         config.max_depth,
		max_files_per_dir: config.max_files_per_dir,
		sort_order:        config.sort_order,
	};
	self::entry(entry_idx, &mut args).with_context(|| format!("While analyzing {}", names[name_range].display()))
}

/// Displays an entry
fn display_entry<W: io::Write>(entry_idx: usize, args: &mut Args<W>) -> Result<(), AppError> {
	let entry = &args.entries[entry_idx];
	let is_at_max_depth = args
		.max_depth
		.is_some_and(|max_depth| args.tree_display.depth() >= max_depth);

	let has_children = match entry {
		Entry::Import(entry) => entry.import.has_children(),
		Entry::Group(_) => false,
	};
	args.tree_display
		.write_prefix(args.output, has_children && !is_at_max_depth)?;

	match &entry {
		Entry::Import(entry) => write!(args.output, "{}:", args.names[entry.name_range].display().bold())?,
		Entry::Group(entry) => write!(args.output, "{}", format_args!("{} other:", entry.len).white().dimmed())?,
	}

	// Shared statistics
	let stats = entry.stats();
	write!(
		args.output,
		" {} ({} on disk)",
		util::fmt_size(stats.size).green().bold(),
		util::fmt_size(BLOCK_SIZE * stats.blocks).green(),
	)?;

	// File specific statistics
	let files = match entry {
		// Note: `-1` to exclude the directory itself, which is included in the count
		Entry::Import(entry) if entry.import.kind.is_dir() => Some(stats.files - 1),
		Entry::Import(_) => None,

		// Note: We only want the recursive files, so ignore the number of files in the group
		Entry::Group(group_entry) => match group_entry.stats.files == group_entry.len {
			// Note: If there are no inner files, don't display any number of files because it'd
			//       be confusing to display `(0 files)`
			true => None,
			false => Some(group_entry.stats.files - group_entry.len),
		},
	};

	if let Some(files) = files {
		write!(args.output, " ({} files)", files.blue())?;
	}

	writeln!(args.output)?;
	Ok(())
}

/// Visits an entry recursively.
///
/// Returns the stats of the entry (recursively)
fn entry<W: io::Write>(entry_idx: usize, args: &mut Args<'_, W>) -> Result<(), AppError> {
	let entry = &args.entries[entry_idx];

	let is_at_max_depth = args
		.max_depth
		.is_some_and(|max_depth| args.tree_display.depth() >= max_depth);

	if let Entry::Import(entry) = entry &&
		let import::DirEntryKind::Dir(dir) = entry.import.kind
	{
		match is_at_max_depth {
			true => {
				args.importer
					.skip_dir_entries(dir)
					.context("Unable to skip directory entries")?;
				let _ = args
					.importer
					.read_dir_footer()
					.context("Unable to read directory footer")?
					.stats;
			},
			false => {
				let names_start_idx = args.names.len();
				let entries_start_idx = args.entries.len();
				for _ in 0..dir.entries_len {
					let entry = entry::read_from_import(entry::Config {
						importer: args.importer,
						names:    args.names,
					})?;

					args.entries.push(Entry::Import(entry));
				}
				let mut entries_idx = range::Range::from(entries_start_idx..args.entries.len());

				// Note: We reverse the sort order here because we need to group the entries
				//       that are smallest, and because we're working with a stack, we can only
				//       pop the largest ones in a normal sort order.
				//       Later during traversal, we reverse the order to ensure it's in the correct
				//       order when displayed.
				let cur_entries = &mut args.entries[entries_idx];
				args.sort_order.sort_entries_reversed(cur_entries);

				// If we have a max number of files per directory, group all files
				// that don't fit into a single entry.
				if let Some(max_files_per_dir) = args.max_files_per_dir &&
					cur_entries.len() > max_files_per_dir
				{
					let len = (cur_entries.len() - max_files_per_dir) as u64;

					let stats = args
						.entries
						.drain(entries_idx.start + max_files_per_dir..)
						.map(|entry| entry.stats())
						.sum::<Stats>();
					entries_idx.end = entries_idx.start + max_files_per_dir;

					let group_entry = Entry::Group(GroupEntry { len, stats });

					let (Ok(idx) | Err(idx)) = args.entries[entries_idx]
						.binary_search_by(|other| args.sort_order.cmp_entry(&group_entry, other));
					args.entries.insert(entries_idx.start + idx, group_entry);
				}

				// Note: See above on why we're reversing this
				for entry_idx in (entries_idx.start..entries_idx.end).rev() {
					args.tree_display.enter(entries_idx.end - entry_idx - 1);

					// Note: We only need to read from the importer for directories, so
					//       there's no point in seeking to non-directories when recursing.
					if let Entry::Import(entry) = &args.entries[entry_idx] &&
						entry.import.kind.is_dir()
					{
						args.importer.seek_entry(&entry.import)?;
					}

					self::entry(entry_idx, args).with_context(|| match &args.entries[entry_idx] {
						Entry::Import(entry) => format!("While analyzing {}", args.names[entry.name_range].display()),
						Entry::Group(_) => "While analyzing <unknown>".to_owned(),
					})?;

					args.tree_display.leave();
				}

				args.names.truncate(names_start_idx);
				args.entries.truncate(entries_start_idx);
			},
		}
	}

	self::display_entry(entry_idx, args)?;

	Ok(())
}

struct Args<'a, W> {
	output:       &'a mut W,
	importer:     &'a mut Importer,
	tree_display: TreeDisplay,

	names:   &'a mut OsString,
	entries: &'a mut Vec<Entry>,

	max_depth:         Option<usize>,
	max_files_per_dir: Option<usize>,
	sort_order:        SortOrder,
}
