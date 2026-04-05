use crate::error::Result;
use crate::tui;

#[allow(dead_code)]
pub fn run(files: &[String]) -> Result<()> {
    tui::run(files)
}
