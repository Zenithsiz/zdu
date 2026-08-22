//! Color support

use {
	core::sync::atomic::{self, AtomicBool},
	std::fmt,
};

/// Whether colors are enabled
static ENABLED: AtomicBool = AtomicBool::new(true);

/// Disables all colors
pub fn disable() {
	ENABLED.store(false, atomic::Ordering::Release);
}

/// Colorize support
pub trait Colorize: Sized {
	colorize_methods! {
		yellow,
		green,
		red,
		bold,
		blue,
		white,
		dimmed,
	}

	fn color(&self, color: impl owo_colors::DynColor) -> Styled<'_, Self> {
		Styled {
			value: self,
			style: owo_colors::Style::new().color(color),
		}
	}
}

macro colorize_methods($( $method:ident ),* $(,)?) {
	$(
		fn $method(&self) -> Styled<'_, Self> {
			Styled {
				value: self,
				style: owo_colors::Style::new().$method()
			}
		}
	)*
}

impl<T: fmt::Display> Colorize for T {}

pub struct Styled<'a, T> {
	value: &'a T,
	style: owo_colors::Style,
}

impl<T: fmt::Display> fmt::Display for Styled<'_, T> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match ENABLED.load(atomic::Ordering::Acquire) {
			true => fmt::Display::fmt(&self.style.style(self.value), f),
			false => fmt::Display::fmt(self.value, f),
		}
	}
}
