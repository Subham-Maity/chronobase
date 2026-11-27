//! Desktop background tray & OS login startup settings service.

use std::sync::Arc;

use crate::platform;
use crate::store::{AppSettings, JsonStore};
use crate::system::SystemError;

/// Service interface for reading and updating desktop background/startup preferences.
pub trait SettingsService: Send + Sync {
    /// Returns the current [`AppSettings`], reconciling OS startup state.
    fn get(&self) -> AppSettings;

    /// Enables or disables hiding to the system tray on window close.
    fn set_run_in_background(&self, enabled: bool) -> Result<AppSettings, SystemError>;

    /// Registers or removes ChronoBase from OS login startup (`--tray`).
    fn set_run_on_startup(&self, enabled: bool) -> Result<AppSettings, SystemError>;
}

/// Default [`SettingsService`] backed by [`JsonStore`] and OS startup helpers.
pub struct DefaultSettingsService {
    store: Arc<JsonStore>,
}

impl DefaultSettingsService {
    /// Creates a new [`DefaultSettingsService`].
    pub fn new(store: Arc<JsonStore>) -> Self {
        Self { store }
    }
}

impl SettingsService for DefaultSettingsService {
    fn get(&self) -> AppSettings {
        let mut settings = self.store.read_with(|d| d.settings.clone());
        settings.run_on_startup = platform::is_autostart_enabled();
        settings
    }

    fn set_run_in_background(&self, enabled: bool) -> Result<AppSettings, SystemError> {
        let updated = self.store.mutate_with(|d| {
            d.settings.run_in_background = enabled;
            d.settings.clone()
        });
        Ok(updated)
    }

    fn set_run_on_startup(&self, enabled: bool) -> Result<AppSettings, SystemError> {
        platform::set_autostart(enabled)?;
        let updated = self.store.mutate_with(|d| {
            d.settings.run_on_startup = enabled;
            d.settings.clone()
        });
        Ok(updated)
    }
}
