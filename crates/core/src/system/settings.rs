//! Desktop background tray & OS login startup settings service.

use std::sync::Arc;

use tokio::sync::Notify;

use crate::platform;
use crate::retention::{CleanupFrequency, RetentionPolicy};
use crate::schedule::parse_cron_expr;
use crate::store::{AppSettings, JsonStore};
use crate::system::SystemError;

/// Service interface for reading and updating desktop background, startup, and retention preferences.
pub trait SettingsService: Send + Sync {
    /// Returns the current [`AppSettings`], reconciling OS startup state.
    fn get(&self) -> AppSettings;

    /// Enables or disables hiding to the system tray on window close.
    fn set_run_in_background(&self, enabled: bool) -> Result<AppSettings, SystemError>;

    /// Registers or removes ChronoBase from OS login startup (`--tray`).
    fn set_run_on_startup(&self, enabled: bool) -> Result<AppSettings, SystemError>;

    /// Updates global backup retention policy & cleanup frequency, waking [`crate::schedule::ScheduleRunner`].
    fn update_retention(
        &self,
        retention: RetentionPolicy,
        cleanup_frequency: CleanupFrequency,
    ) -> Result<AppSettings, SystemError>;
}

/// Default [`SettingsService`] backed by [`JsonStore`] and OS startup helpers.
pub struct DefaultSettingsService {
    store: Arc<JsonStore>,
    schedule_reload: Option<Arc<Notify>>,
}

impl DefaultSettingsService {
    /// Creates a new [`DefaultSettingsService`].
    pub fn new(store: Arc<JsonStore>) -> Self {
        Self {
            store,
            schedule_reload: None,
        }
    }

    /// Attaches a [`Notify`] handle to wake [`crate::schedule::ScheduleRunner`] when retention frequency changes.
    pub fn with_schedule_reload(mut self, reload_notify: Arc<Notify>) -> Self {
        self.schedule_reload = Some(reload_notify);
        self
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

    fn update_retention(
        &self,
        retention: RetentionPolicy,
        cleanup_frequency: CleanupFrequency,
    ) -> Result<AppSettings, SystemError> {
        if let CleanupFrequency::Custom(ref expr) = cleanup_frequency {
            parse_cron_expr(expr).map_err(|e| SystemError::Validation(e.to_string()))?;
        }
        let updated = self.store.mutate_with(|d| {
            d.settings.retention = retention;
            d.settings.cleanup_frequency = cleanup_frequency;
            d.settings.clone()
        });
        if let Some(ref notify) = self.schedule_reload {
            notify.notify_one();
        }
        Ok(updated)
    }
}
