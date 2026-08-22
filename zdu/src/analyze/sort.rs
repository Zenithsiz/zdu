//! Entry sorting

use {super::entry::Entry, crate::args, core::cmp};


/// Sort order
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub struct SortOrder {
	pub kind: SortOrderKind,

	/// Whether to sort directories before files.
	///
	/// If `false`, they are sorted in-between.
	pub dirs_before: bool,

	pub reverse: bool,
}

/// Sort order kind
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum SortOrderKind {
	Size,
	Blocks,
	Files,
}

impl SortOrder {
	/// Creates a sort order from clap flags
	// TODO: Create a sub-parser for all these flags and flatten it?
	pub fn new(sort: args::SortOrder, sort_directories: bool, sort_reverse: bool) -> Self {
		SortOrder {
			kind:        match sort {
				args::SortOrder::Size => SortOrderKind::Size,
				args::SortOrder::Blocks => SortOrderKind::Blocks,
				args::SortOrder::Files => SortOrderKind::Files,
			},
			dirs_before: sort_directories,
			reverse:     sort_reverse,
		}
	}

	/// Compares two entries according to this sort order, except for reversing
	fn cmp_entry_no_reverse(&self, lhs: &Entry, rhs: &Entry) -> cmp::Ordering {
		if self.dirs_before {
			match (lhs.is_group_or_dir(), rhs.is_group_or_dir()) {
				(true, false) => return cmp::Ordering::Greater,
				(false, true) => return cmp::Ordering::Less,
				(true, true) | (false, false) => (),
			}
		}

		let lhs_stats = lhs.stats();
		let rhs_stats = rhs.stats();
		match self.kind {
			SortOrderKind::Size => u64::cmp(&lhs_stats.size, &rhs_stats.size),
			SortOrderKind::Blocks => u64::cmp(&lhs_stats.blocks, &rhs_stats.blocks),
			SortOrderKind::Files => u64::cmp(&lhs_stats.files, &rhs_stats.files),
		}
	}

	/// Compares two entries according to this sort order
	pub fn cmp_entry(&self, lhs: &Entry, rhs: &Entry) -> cmp::Ordering {
		let cmp = self.cmp_entry_no_reverse(lhs, rhs);
		match self.reverse {
			true => cmp.reverse(),
			false => cmp,
		}
	}

	/// Sorts entries according to this sort order reversed.
	pub fn sort_entries_reversed(self, entries: &mut [Entry]) {
		entries.sort_by(|lhs, rhs| self.cmp_entry(lhs, rhs).reverse());
	}
}
