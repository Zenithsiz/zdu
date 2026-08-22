//! Compare two imports

// TODO: Sort entries by total size added/removed?.

use {
	crate::{
		BLOCK_SIZE,
		archive::import::{self, Importer},
		stats::{Stats, StatsDiff},
		tree_display::TreeDisplay,
		util::{self, AppError, Colorize as _, OsStrAsPath, OsStrRange},
	},
	core::{cmp, fmt, range},
	owo_colors::AnsiColors,
	std::{
		ffi::{OsStr, OsString},
		io,
	},
};

/// Configuration
pub struct Config<'a, W> {
	pub output:           &'a mut W,
	pub lhs_importer:     &'a mut Importer,
	pub rhs_importer:     &'a mut Importer,
	pub lhs_entry_import: import::DirEntry,
	pub rhs_entry_import: import::DirEntry,

	pub lhs_entry_name: &'a OsStr,
	pub rhs_entry_name: &'a OsStr,

	pub max_depth:      Option<usize>,
	pub show_unchanged: bool,
}

/// Compares an entry
pub fn compare<W: io::Write>(config: Config<'_, W>) -> Result<(), AppError> {
	let mut names = OsString::with_capacity(config.lhs_entry_name.len() + config.rhs_entry_name.len());
	names.push(config.lhs_entry_name);
	names.push(config.rhs_entry_name);
	let lhs_name_range = 0..config.lhs_entry_name.len();
	let rhs_name_range = lhs_name_range.end..lhs_name_range.end + config.rhs_entry_name.len();

	let mut entries = vec![];
	entries.push(Entry {
		name_range: OsStrRange::from(lhs_name_range),
		stats:      config.lhs_importer.read_entry_stats(config.lhs_entry_import)?,
		import:     config.lhs_entry_import,
	});
	let lhs_entry_idx = 0;
	entries.push(Entry {
		name_range: OsStrRange::from(rhs_name_range),
		stats:      config.rhs_importer.read_entry_stats(config.rhs_entry_import)?,
		import:     config.rhs_entry_import,
	});
	let rhs_entry_idx = 1;

	let mut args = Args {
		tree_display: TreeDisplay::new(),
		names,
		entries,
		max_depth: config.max_depth,
		show_unchanged: config.show_unchanged,
	};
	self::entry(
		config.output,
		config.lhs_importer,
		config.rhs_importer,
		lhs_entry_idx,
		rhs_entry_idx,
		&mut args,
	)?;
	Ok(())
}

/// Compares changes
///
/// Returns if this entry was displayed or not
fn entry_changes<W: io::Write>(
	output: &mut W,
	changes: Changes,
	has_children: bool,
	args: &mut Args,
) -> Result<bool, AppError> {
	if args
		.max_depth
		.is_some_and(|max_depth| args.tree_display.depth() > max_depth)
	{
		return Ok(false);
	}

	match changes {
		Changes::Changed {
			lhs_entry_name,
			rhs_entry_name,
			diff,
		} => {
			let lhs_entry_name = &args.names[lhs_entry_name];
			let rhs_entry_name = &args.names[rhs_entry_name];

			let (size_sign, size_color, size) = match u64::cmp(&diff.remove.size, &diff.add.size) {
				cmp::Ordering::Less => ("+", AnsiColors::Green, diff.add.size - diff.remove.size),
				// Note: When equal, we still want to display if either:
				//       - The actual added/removed isn't 0
				//       - The user wants to show unchanged entries
				//       - We are the root entry
				cmp::Ordering::Equal =>
					match diff != StatsDiff::default() || args.show_unchanged || args.tree_display.depth() == 0 {
						true => ("", AnsiColors::Yellow, 0),
						false => return Ok(false),
					},
				cmp::Ordering::Greater => ("-", AnsiColors::Red, diff.remove.size - diff.add.size),
			};
			let (blocks_sign, blocks_color, blocks) = match u64::cmp(&diff.remove.blocks, &diff.add.blocks) {
				cmp::Ordering::Less => ("+", AnsiColors::Green, diff.add.blocks - diff.remove.blocks),
				cmp::Ordering::Equal => ("", AnsiColors::Yellow, 0),
				cmp::Ordering::Greater => ("-", AnsiColors::Red, diff.remove.blocks - diff.add.blocks),
			};

			let files = fmt::from_fn(|f| match (diff.add.files, diff.remove.files) {
				(0, 0) => write!(f, "{}", 0.yellow()),
				(_, 0) => write!(f, "+{}", diff.add.files.green()),
				(0, _) => write!(f, "-{}", diff.remove.files.red()),
				(..) => write!(f, "+{}/-{}", diff.add.files.green(), diff.remove.files.red()),
			});

			super let size = format_args!("{size_sign}{}", util::fmt_size(size)).color(size_color);
			super let blocks_size =
				format_args!("{blocks_sign}{}", util::fmt_size(BLOCK_SIZE * blocks)).color(blocks_color);

			args.tree_display.write_prefix(output, has_children)?;

			match lhs_entry_name == rhs_entry_name {
				true => write!(output, "{}: ", lhs_entry_name.as_path().display())?,
				false => {
					let (common_prefix, lhs, rhs) = util::common_os_str_prefix(lhs_entry_name, rhs_entry_name);
					let (lhs, rhs, common_suffix) = util::common_os_str_suffix(lhs, rhs);

					match (lhs.is_empty(), rhs.is_empty()) {
						(true, true) => write!(output, "{}{}: ", common_prefix.display(), common_suffix.display())?,
						(true, false) => write!(
							output,
							"{}{{{}}}{}: ",
							common_prefix.display(),
							rhs.display(),
							common_suffix.display()
						)?,
						(false, true) => write!(
							output,
							"{}{{{}}}{}: ",
							common_prefix.display(),
							lhs.display(),
							common_suffix.display()
						)?,
						(false, false) => write!(
							output,
							"{}{{{},{}}}{}: ",
							common_prefix.display(),
							lhs.display(),
							rhs.display(),
							common_suffix.display(),
						)?,
					}
				},
			}
			write!(output, "{size} ({blocks_size} on disk) ({files} files)")?
		},
		Changes::Removed { lhs_entry_name, stats } => {
			let lhs_entry_name = &args.names[lhs_entry_name];
			args.tree_display.write_prefix(output, has_children)?;

			write!(
				output,
				"-{}: {} ({} on disk)",
				lhs_entry_name.as_path().display().red(),
				util::fmt_size(stats.size).green().bold(),
				util::fmt_size(BLOCK_SIZE * stats.blocks).green(),
			)?
		},
		Changes::Added { rhs_entry_name, stats } => {
			let rhs_entry_name = &args.names[rhs_entry_name];
			args.tree_display.write_prefix(output, has_children)?;

			write!(
				output,
				"+{}: {} ({} on disk)",
				rhs_entry_name.as_path().display().green(),
				util::fmt_size(stats.size).green().bold(),
				util::fmt_size(BLOCK_SIZE * stats.blocks).green(),
			)?
		},
	}

	writeln!(output)?;
	Ok(true)
}

/// Compares directory entries
fn dir_entries<W: io::Write>(
	output: &mut W,
	lhs_importer: &mut Importer,
	rhs_importer: &mut Importer,
	lhs_dir: import::DirHeader,
	rhs_dir: import::DirHeader,
	args: &mut Args,
) -> Result<StatsDiff, AppError> {
	let mut entries_diff = StatsDiff::default();

	let names_start_idx = args.names.len();
	let entries_start_idx = args.entries.len();
	let (_, lhs_entries_idx) = Entry::read_dir(lhs_importer, lhs_dir, &mut args.entries, &mut args.names)?;
	let (_, rhs_entries_idx) = Entry::read_dir(rhs_importer, rhs_dir, &mut args.entries, &mut args.names)?;

	args.entries[lhs_entries_idx].sort_by(|lhs, rhs| args.names[lhs.name_range].cmp(&args.names[rhs.name_range]));
	args.entries[rhs_entries_idx].sort_by(|lhs, rhs| args.names[lhs.name_range].cmp(&args.names[rhs.name_range]));

	let mut cur_entries_idx = 0;
	let mut cur_lhs_idx = 0;
	let mut cur_rhs_idx = 0;
	loop {
		args.tree_display.enter(cur_entries_idx);

		let lhs_entry = args.entries[lhs_entries_idx].get(cur_lhs_idx);
		let rhs_entry = args.entries[rhs_entries_idx].get(cur_rhs_idx);

		let entries = match (lhs_entry, rhs_entry) {
			(Some(lhs_entry), Some(rhs_entry)) =>
				match args.names[lhs_entry.name_range].cmp(&args.names[rhs_entry.name_range]) {
					cmp::Ordering::Less => itertools::EitherOrBoth::Left(lhs_entry),
					cmp::Ordering::Equal => itertools::EitherOrBoth::Both(lhs_entry, rhs_entry),
					cmp::Ordering::Greater => itertools::EitherOrBoth::Right(rhs_entry),
				},
			(Some(lhs_entry), None) => itertools::EitherOrBoth::Left(lhs_entry),
			(None, Some(rhs_entry)) => itertools::EitherOrBoth::Right(rhs_entry),
			(None, None) => {
				args.tree_display.leave();
				break;
			},
		};

		match entries {
			itertools::EitherOrBoth::Both(..) => {
				let (diff, displayed) = self::entry(
					output,
					lhs_importer,
					rhs_importer,
					lhs_entries_idx.start + cur_lhs_idx,
					rhs_entries_idx.start + cur_rhs_idx,
					args,
				)?;

				if displayed {
					cur_entries_idx += 1;
				}

				entries_diff += diff;

				cur_lhs_idx += 1;
				cur_rhs_idx += 1;
			},
			itertools::EitherOrBoth::Left(lhs_entry) => {
				entries_diff.remove += lhs_entry.stats;

				let changes = Changes::Removed {
					lhs_entry_name: lhs_entry.name_range,
					stats:          lhs_entry.stats,
				};
				if self::entry_changes(output, changes, false, args)? {
					cur_entries_idx += 1;
				}

				cur_lhs_idx += 1;
			},
			itertools::EitherOrBoth::Right(rhs_entry) => {
				entries_diff.add += rhs_entry.stats;

				let changes = Changes::Added {
					rhs_entry_name: rhs_entry.name_range,
					stats:          rhs_entry.stats,
				};
				if self::entry_changes(output, changes, false, args)? {
					cur_entries_idx += 1;
				};

				cur_rhs_idx += 1;
			},
		}

		args.tree_display.leave();
	}

	args.names.truncate(names_start_idx);
	args.entries.truncate(entries_start_idx);

	Ok(entries_diff)
}

fn entry<W: io::Write>(
	output: &mut W,
	lhs_importer: &mut Importer,
	rhs_importer: &mut Importer,
	lhs_entry_idx: usize,
	rhs_entry_idx: usize,
	args: &mut Args,
) -> Result<(StatsDiff, bool), AppError> {
	let [lhs_entry, rhs_entry] = args
		.entries
		.get_disjoint_mut([lhs_entry_idx, rhs_entry_idx])
		.expect("Indices should be disjoint");

	let has_children = lhs_entry.import.has_children() &&
		rhs_entry.import.has_children() &&
		args.max_depth
			.is_none_or(|max_depth| args.tree_display.depth() < max_depth);

	let mut diff = StatsDiff::default();

	if let (import::DirEntryKind::Dir(lhs_dir), import::DirEntryKind::Dir(rhs_dir)) =
		(lhs_entry.import.kind, rhs_entry.import.kind)
	{
		lhs_importer.seek_entry(&lhs_entry.import)?;
		rhs_importer.seek_entry(&rhs_entry.import)?;
		let entries_diff = self::dir_entries(output, lhs_importer, rhs_importer, lhs_dir, rhs_dir, args)?;

		diff += entries_diff;
	}

	let displayed = self::entry_changes(
		output,
		Changes::Changed {
			lhs_entry_name: args.entries[lhs_entry_idx].name_range,
			rhs_entry_name: args.entries[rhs_entry_idx].name_range,
			diff,
		},
		has_children,
		args,
	)?;

	Ok((diff, displayed))
}

#[derive(Debug)]
struct Entry {
	name_range: OsStrRange,
	stats:      Stats,
	import:     import::DirEntry,
}

impl Entry {
	/// Reads all entries in a directory into a Cbs.
	///
	/// Returns the total stats of the entries and the range of entries read
	pub fn read_dir(
		importer: &mut Importer,
		dir: import::DirHeader,
		entries: &mut Vec<Self>,
		names: &mut OsString,
	) -> Result<(Stats, range::Range<usize>), AppError> {
		let entries_start_idx = entries.len();
		for _ in 0..dir.entries_len {
			let (import, name_range) = importer.read_entry(names)?;
			let stats = importer.read_entry_stats(import)?;
			entries.push(Entry {
				name_range,
				stats,
				import,
			})
		}
		let range = (entries_start_idx..entries.len()).into();

		let stats = importer.read_dir_footer()?.stats;

		Ok((stats, range))
	}
}

/// Arguments
#[derive(Debug)]
struct Args {
	tree_display: TreeDisplay,

	names:   OsString,
	entries: Vec<Entry>,

	max_depth:      Option<usize>,
	show_unchanged: bool,
}

#[derive(Debug)]
enum Changes {
	Changed {
		lhs_entry_name: OsStrRange,
		rhs_entry_name: OsStrRange,
		diff:           StatsDiff,
	},
	Added {
		rhs_entry_name: OsStrRange,
		stats:          Stats,
	},
	Removed {
		lhs_entry_name: OsStrRange,
		stats:          Stats,
	},
}
