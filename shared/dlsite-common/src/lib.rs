#![no_std]

pub mod api;
pub mod explore;
pub mod filters;
pub mod home;
pub mod models;
pub mod settings;

/// Print only when the "Debug Logging" switch is enabled in source settings.
#[macro_export]
macro_rules! debug_print {
	($($arg:tt)*) => {
		if $crate::settings::is_debug_logging_enabled() {
			aidoku::imports::std::print(aidoku::alloc::format!($($arg)*));
		}
	};
}
