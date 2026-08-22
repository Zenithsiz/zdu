//! Tree display

use std::io;

/// Tree display
#[derive(Debug)]
pub struct TreeDisplay {
	idxs: Vec<usize>,
}

impl TreeDisplay {
	/// Creates a new, empty, tree display with no indentation
	pub fn new() -> Self {
		Self { idxs: vec![] }
	}

	/// Returns the current depth of this tree display
	pub fn depth(&self) -> usize {
		self.idxs.len()
	}

	/// Enters a new depth with an index.
	pub fn enter(&mut self, idx: usize) {
		self.idxs.push(idx);
	}

	/// Leaves the current depth
	pub fn leave(&mut self) {
		self.idxs.pop();
	}

	/// Writes this tree's prefix to `output`
	pub fn write_prefix<W: io::Write>(&self, output: &mut W, has_children: bool) -> Result<(), app_error::AppError> {
		let mut idxs = self.idxs.iter().peekable();

		let mut cur_depth = 1;
		while let Some(idx) = idxs.next() {
			match idxs.peek().is_some() {
				true => match idx {
					0 => write!(output, "  ")?,
					_ => write!(output, "│ ")?,
				},
				false => match idx {
					0 => write!(output, "┌─")?,
					_ => write!(output, "├─")?,
				},
			}
			cur_depth += 1;
		}

		if cur_depth > 1 {
			match has_children {
				true => write!(output, "┴╴")?,
				false => write!(output, "─╴")?,
			}
		}

		Ok(())
	}
}
