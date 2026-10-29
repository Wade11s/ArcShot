//! Text-only runtime diagnostics; never includes captured pixels or encoded images.
use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};

static LOGGER: StderrLog = StderrLog;
struct StderrLog;
impl Log for StderrLog {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Info
    }
    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!(
                "level={} target={} {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }
    fn flush(&self) {}
}
pub(crate) fn init() -> Result<(), SetLoggerError> {
    log::set_logger(&LOGGER)?;
    log::set_max_level(LevelFilter::Info);
    Ok(())
}
