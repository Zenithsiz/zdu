//! Arguments

use {clap::ValueHint, std::path::PathBuf};

/// Zenithsiz's Disk Usage
///
/// Calculates the disk usage of a directory, saving it to disk.
#[derive(Debug, clap::Parser)]
pub struct Args {
	/// Command
	#[clap(subcommand)]
	pub command: Subcommand,

	/// File to log into.
	///
	/// You can control the logging level with the `RUST_FILE_LOG`
	/// environment variable
	#[clap(long)]
	pub log_file: Option<PathBuf>,
}

#[derive(Debug, clap::Subcommand)]
pub enum Subcommand {
	/// Scan the filesystem and export the results
	Scan {
		/// Directory to scan
		#[clap(value_hint = ValueHint::DirPath)]
		scan_dir: PathBuf,

		/// Results export
		#[clap(long, value_hint = ValueHint::FilePath)]
		export: PathBuf,

		/// Don't leave the root filesystem
		#[clap(long)]
		same_filesystem: bool,

		/// Resolve all symlinks
		#[clap(long)]
		follow_symlinks: bool,

		/// Ignore the following paths.
		///
		/// They may be relative to the root (not the current directory!),
		/// or absolute paths.
		#[clap(long, value_hint = ValueHint::AnyPath)]
		ignore: Vec<PathBuf>,
	},

	/// Analyze exported results
	Analyze {
		/// Results import
		#[clap(value_hint = ValueHint::FilePath)]
		import: PathBuf,

		/// Max depth
		#[clap(long)]
		max_depth: Option<usize>,

		/// Maximum files per directory
		#[clap(long)]
		max_files_per_dir: Option<usize>,

		/// Sort files.
		#[clap(long, default_value = "size")]
		sort: SortOrder,

		/// Reverse sort order.
		///
		/// Default is smallest to largest
		#[clap(long)]
		sort_reverse: bool,

		/// Sort directories before files.
		///
		/// By default, they are sorted in between files.
		#[clap(long)]
		sort_directories: bool,

		/// Path to analyze.
		///
		/// May be relative to root, or absolute.
		///
		/// If not specified, the root path is analyzed
		#[clap(value_hint = ValueHint::FilePath)]
		path: Option<PathBuf>,
	},

	/// Find duplicates
	FindDuplicates {
		/// Results import
		#[clap(value_hint = ValueHint::FilePath)]
		import: PathBuf,

		/// Path to find at.
		///
		/// May be relative to root, or absolute.
		///
		/// If not specified, the root path is used
		#[clap(value_hint = ValueHint::FilePath)]
		path: Option<PathBuf>,
	},

	/// Compares two imports
	Compare {
		/// First import
		#[clap(value_hint = ValueHint::FilePath)]
		lhs: PathBuf,

		/// Second import
		#[clap(value_hint = ValueHint::FilePath)]
		rhs: PathBuf,

		/// Path to compare the left import at.
		///
		/// May be relative to root, or absolute if both imports
		/// were scanned from the same path.
		///
		/// If not specified, the root path is used
		#[clap(value_hint = ValueHint::FilePath)]
		lhs_path: Option<PathBuf>,

		/// Path to compare the right import at.
		///
		/// If this isn't specified, but the left import path is
		/// specified, it will be used instead.
		///
		/// May be relative to root, or absolute if both imports
		/// were scanned from the same path.
		///
		/// If not specified, the root path is used
		#[clap(value_hint = ValueHint::FilePath)]
		rhs_path: Option<PathBuf>,

		/// Max depth
		#[clap(long)]
		max_depth: Option<usize>,

		// TODO: Add `max_files_per_dir`?
		/// Show unchanged files
		#[clap(long)]
		show_unchanged: bool,
	},

	/// Generate shell completions
	Complete {
		/// The shell to generate completions for
		#[clap(long)]
		shell: clap_complete::Shell,

		/// Path to write the completions to, or stdout if unspecified
		#[clap(long, value_hint = ValueHint::DirPath)]
		output: Option<PathBuf>,
	},
}

/// Sort order
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum SortOrder {
	Size,
	Blocks,
	Files,
}
