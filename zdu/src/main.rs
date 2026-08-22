//! Zenithsiz's Disk Usage

#![feature(
	path_absolute_method,
	os_str_slice,
	yeet_expr,
	unboxed_closures,
	super_let,
	dir_entry_ext2,
	read_array,
	file_buffered,
	integer_widen_truncate,
	decl_macro,
	os_string_truncate,
	path_trailing_sep,
	read_le
)]

mod analyze;
mod archive;
mod args;
mod compare;
mod find_duplicates;
mod scan;
mod stats;
mod tree_display;
mod util;

use {
	self::{
		archive::{export::Exporter, import::Importer},
		args::Args,
		util::AppError,
	},
	app_error::Context,
	clap::{CommandFactory, Parser},
	std::{
		collections::HashMap,
		env,
		ffi::OsString,
		fs,
		io::{self, IsTerminal},
		os::unix::prelude::MetadataExt as _,
	},
	util::Colorize as _,
};

#[expect(clippy::too_many_lines, reason = "TODO: Refactor")]
fn main() -> Result<(), AppError> {
	let indicatif_layer = tracing_indicatif::IndicatifLayer::new()
		.with_progress_style(
			#[expect(
				clippy::literal_string_with_formatting_args,
				reason = "`indicatif` will resolve these"
			)]
			tracing_indicatif::style::ProgressStyle::default_spinner()
				.progress_chars("█▉▊▋▌▍▎▏ ")
				.template(
					"{span_child_prefix}[{elapsed:>3.green}/{duration:<3}] {span_name:.red} {span_fields:.bold} \
					 ▐{wide_bar:.green}▌ {human_pos:>5}/{human_len:<5}",
				)
				.expect("Progress template should be valid"),
		)
		.with_span_child_prefix_indent(" ")
		.with_span_child_prefix_symbol("↳");

	let logger = zutil_logger::Logger::builder()
		.stderr(indicatif_layer.get_stderr_writer())
		.layer(indicatif_layer)
		.build();


	let args = Args::parse();
	tracing::debug!(?args);

	logger.set_file(args.log_file.as_deref());

	if !io::stdout().is_terminal() || env::var_os("NOCOLOR").is_some_and(|var| !var.is_empty()) {
		util::colors::disable();
	}

	match args.command {
		args::Subcommand::Scan {
			scan_dir,
			export,
			same_filesystem,
			follow_symlinks,
			ignore,
		} => {
			tracing::info!("Exporting to {}", export.display());

			let scan_dir = scan_dir
				.canonicalize()
				.context("Unable to canonicalize scan directory")?;
			let root_metadata = fs::metadata(&scan_dir).context("Unable to get root metadata")?;

			let mut args = scan::Args {
				root_dev: same_filesystem.then(|| root_metadata.dev()),
				follow_symlinks,
				ignore: ignore
					.into_iter()
					.filter_map(|path| match path.absolute() {
						Ok(path) => Some(path),
						Err(err) => {
							tracing::warn!("Unable to make path {path:?} absolute, ignoring it: {err:?}");
							None
						},
					})
					.collect(),
				hardlink_inodes_seen: HashMap::new(),
				depth: 0,
			};

			let mut exporter = Exporter::new(&export).context("Unable to create export")?;
			let root_stats = scan::entry_from_metadata(
				&scan_dir,
				scan_dir.as_os_str(),
				&root_metadata,
				&mut exporter,
				&mut args,
			)
			.context("Unable to export")?
			.context("Root entry wasn't exporter")?;
			exporter.finish().context("Unable to finish export")?;

			println!(
				"{}: {} ({} on disk) ({} files)",
				scan_dir.display().bold(),
				util::fmt_size(root_stats.size).green().bold(),
				util::fmt_size(BLOCK_SIZE * root_stats.blocks).green(),
				root_stats.files.blue(),
			);
		},

		args::Subcommand::Analyze {
			import,
			max_depth,
			max_files_per_dir,
			path,
			sort,
			sort_reverse,
			sort_directories,
		} => {
			let mut importer = Importer::new(&import).context("Unable to open import")?;
			let mut root_path = OsString::new();
			let (mut root_entry, _) = importer
				.read_entry(&mut root_path)
				.context("Unable to read import root")?;

			if let Some(path) = &path {
				(root_entry, root_path) = importer.seek_path(root_entry, root_path, path)?;
			}

			let mut stdout = io::stdout().lock();
			let sort_order = analyze::sort::SortOrder::new(sort, sort_directories, sort_reverse);
			analyze::analyze(analyze::Config {
				output: &mut stdout,
				importer: &mut importer,
				entry: root_entry,
				entry_name: root_path,
				max_depth,
				max_files_per_dir,
				sort_order,
			})
			.context("Unable to analyze entry")?;
		},

		args::Subcommand::FindDuplicates { import, path } => {
			let mut importer = Importer::new(&import).context("Unable to open import")?;
			let mut root_path = OsString::new();
			let (mut root_entry, _) = importer
				.read_entry(&mut root_path)
				.context("Unable to read import root")?;

			if let Some(path) = &path {
				(root_entry, root_path) = importer.seek_path(root_entry, root_path, path)?;
			}

			let output = find_duplicates::find_duplicates(find_duplicates::Config {
				importer:     &mut importer,
				entry_import: root_entry,
				entry_path:   root_path,
			})?;

			let mut total_size = 0;
			let mut files_by_size = output.files_by_size.into_iter().collect::<Vec<_>>();
			files_by_size.sort_by_key(|&(size, _)| size);
			for &(size, ref files_by_size) in &files_by_size {
				if files_by_size.last_unread.is_some() {
					assert!(files_by_size.files_by_md5.is_empty());
					continue;
				}

				for (digest, files) in &files_by_size.files_by_md5 {
					if files.len() <= 1 {
						continue;
					}

					total_size += (files.len() as u64 - 1) * size;

					println!("=== {:?} ({})", digest, util::fmt_size(size).green().bold());
					for file in files {
						println!("{}", file.display());
					}
				}
			}

			tracing::debug!("Found {} unique file sizes", files_by_size.len());
			tracing::debug!(
				"Found {} unique file contents",
				files_by_size
					.iter()
					.map(|(_, files_by_size)| files_by_size.files_by_md5.len())
					.sum::<usize>()
			);
			tracing::info!(
				"Read {}/{} files ({}/{}) to find duplicates",
				output.files_read,
				output.files_total,
				util::fmt_size(output.size_read),
				util::fmt_size(output.size_total)
			);
			tracing::info!("Duplicate file overhead {}", util::fmt_size(total_size));
		},

		args::Subcommand::Compare {
			lhs,
			rhs,
			lhs_path,
			rhs_path,
			max_depth,
			show_unchanged,
		} => {
			let mut lhs_importer = Importer::new(&lhs).context("Unable to open import")?;
			let mut lhs_root_path = OsString::new();
			let (mut lhs_root_entry, _) = lhs_importer
				.read_entry(&mut lhs_root_path)
				.context("Unable to read import root")?;

			let mut rhs_importer = Importer::new(&rhs).context("Unable to open import")?;
			let mut rhs_root_path = OsString::new();
			let (mut rhs_root_entry, _) = rhs_importer
				.read_entry(&mut rhs_root_path)
				.context("Unable to read import root")?;

			if let Some(lhs_path) = &lhs_path {
				(lhs_root_entry, lhs_root_path) = lhs_importer.seek_path(lhs_root_entry, lhs_root_path, lhs_path)?;
			}
			if let Some(rhs_path) = rhs_path.as_ref().or(lhs_path.as_ref()) {
				(rhs_root_entry, rhs_root_path) = rhs_importer.seek_path(rhs_root_entry, rhs_root_path, rhs_path)?;
			}

			let mut stdout = io::stdout().lock();
			compare::compare(compare::Config {
				output: &mut stdout,
				lhs_importer: &mut lhs_importer,
				rhs_importer: &mut rhs_importer,
				lhs_entry_import: lhs_root_entry,
				rhs_entry_import: rhs_root_entry,
				lhs_entry_name: &lhs_root_path,
				rhs_entry_name: &rhs_root_path,
				max_depth,
				show_unchanged,
			})?;
		},

		args::Subcommand::Complete { shell, output } => {
			let mut cmd = Args::command();
			let bin_name = "zdu";

			match output {
				Some(output_dir) => {
					let output = clap_complete::generate_to(shell, &mut cmd, bin_name, output_dir)
						.context("Unable to generate completions")?;
					tracing::info!("Wrote completions to {output:?}");
				},
				None => clap_complete::generate(shell, &mut cmd, bin_name, &mut std::io::stdout()),
			}
		},
	}

	Ok(())
}

const BLOCK_SIZE: u64 = 512;
