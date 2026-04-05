use crate::error::Result;
use crate::tui;

pub fn run(files: &[String]) -> Result<()> {
    tui::run(files)
}
