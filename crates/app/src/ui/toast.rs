//! Toast notification helper with 3.8s auto-dismiss via `slint::Timer`.

use std::rc::Rc;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

use slint::{Model, VecModel, Weak};

use crate::ui::{MainWindow, ToastMessage};

static NEXT_TOAST_ID: AtomicI32 = AtomicI32::new(1);

/// Semantic toast severity.
#[derive(Debug, Clone, Copy)]
pub enum ToastKind {
    Success,
    Error,
    Info,
}

impl ToastKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Error => "error",
            Self::Info => "info",
        }
    }
}

/// Pushes a toast notification on the Slint UI thread and schedules removal after 3.8s.
pub fn show_toast(weak: &Weak<MainWindow>, message: String, kind: ToastKind) {
    let weak_clone = weak.clone();
    // Intentionally ignore: UI may have already closed.
    let _ = slint::invoke_from_event_loop(move || {
        let Some(win) = weak_clone.upgrade() else {
            return;
        };
        let id = NEXT_TOAST_ID.fetch_add(1, Ordering::Relaxed);
        let model = win.get_toasts();
        let vec_model = if let Some(vm) = model.as_any().downcast_ref::<VecModel<ToastMessage>>() {
            vm.push(ToastMessage {
                id,
                text: message.into(),
                kind: kind.as_str().into(),
            });
            None
        } else {
            let vm = Rc::new(VecModel::from(vec![ToastMessage {
                id,
                text: message.into(),
                kind: kind.as_str().into(),
            }]));
            Some(vm)
        };
        if let Some(vm) = vec_model {
            win.set_toasts(vm.into());
        }

        let weak_rm = weak_clone.clone();
        slint::Timer::single_shot(Duration::from_millis(3800), move || {
            let Some(win) = weak_rm.upgrade() else {
                return;
            };
            let model = win.get_toasts();
            if let Some(vm) = model.as_any().downcast_ref::<VecModel<ToastMessage>>() {
                for i in 0..vm.row_count() {
                    if let Some(row) = vm.row_data(i)
                        && row.id == id
                    {
                        vm.remove(i);
                        break;
                    }
                }
            }
        });
    });
}
