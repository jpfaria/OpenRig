//! Responsibility: runs the plugin editor window over one draft at a time.
//!
//! A save of an edit, a restore and the naming of a TONE3000 tone answer at
//! once and close or reload the window. A create and a TONE3000 redo answer
//! on a later drain: the window stays busy until `on_events` sees the
//! answer, and while it waits the tick drains the dispatcher itself. A tone
//! that waits for names opens the editor on the next tick.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use application::command::{Command, PluginLibraryCommand};
use application::event::{Event, PluginLibraryEvent};
use application::plugin_library::PluginOrigin;
use slint::Global;

use crate::plugin_editor_bridge_sync::{set_editor_view, EditorChrome};
use crate::plugin_editor_draft::{EditorDraft, EditorMode};
use crate::plugin_library_session::{dispatch_library, plugin_entries, plugin_grid_of};
use crate::tone3000_ctx::ForwardEvents;
use crate::tone3000_session::{poll_tone3000, tone3000_snapshot, SessionCell};
use crate::{PluginEditorBridge, PluginEditorWindow};

/// Shows or hides the editor window.
pub(crate) type WindowToggle = Rc<dyn Fn()>;

#[derive(Default)]
struct Header {
    subject: String,
    tone3000: bool,
}

#[derive(Clone)]
pub(crate) struct PluginEditorCtx {
    project_session: SessionCell,
    window: slint::Weak<PluginEditorWindow>,
    draft: Rc<RefCell<Option<EditorDraft>>>,
    header: Rc<RefCell<Header>>,
    /// A create or a redo is waiting for its answer.
    busy: Rc<Cell<bool>>,
    error: Rc<RefCell<String>>,
    forward: ForwardEvents,
    show: WindowToggle,
    hide: WindowToggle,
}

impl PluginEditorCtx {
    pub(crate) fn new(
        project_session: SessionCell,
        window: slint::Weak<PluginEditorWindow>,
        forward: ForwardEvents,
        show: WindowToggle,
        hide: WindowToggle,
    ) -> Self {
        Self {
            project_session,
            window,
            draft: Rc::new(RefCell::new(None)),
            header: Rc::new(RefCell::new(Header::default())),
            busy: Rc::new(Cell::new(false)),
            error: Rc::new(RefCell::new(String::new())),
            forward,
            show,
            hide,
        }
    }

    /// Open an owned plugin's parameters; `Err` says why it cannot be edited.
    pub(crate) fn open_edit(&self, plugin_id: &str) -> Result<(), String> {
        let Some(view) = plugin_grid_of(&self.project_session, plugin_id) else {
            return Ok(());
        };
        let draft = EditorDraft::edit(view?)
            .ok_or_else(|| format!("`{plugin_id}` has no captures to edit"))?;
        let entry = plugin_entries(&self.project_session)
            .unwrap_or_default()
            .into_iter()
            .find(|e| e.plugin_id == plugin_id);
        *self.header.borrow_mut() = Header {
            subject: entry
                .as_ref()
                .map_or_else(|| plugin_id.to_string(), |e| e.display_name.clone()),
            tone3000: entry.is_some_and(|e| e.origin == PluginOrigin::Tone3000),
        };
        self.open(draft);
        Ok(())
    }

    /// Open an empty draft for a new capture plugin.
    pub(crate) fn open_create(&self) {
        *self.header.borrow_mut() = Header::default();
        self.open(EditorDraft::create());
    }

    /// Change the draft. `republish` redraws the window: a structural change
    /// does, a typed value does not, so the field keeps its focus.
    pub(crate) fn edit(&self, republish: bool, change: impl FnOnce(&mut EditorDraft)) {
        if let Some(draft) = self.draft.borrow_mut().as_mut() {
            change(draft);
        }
        if republish {
            self.publish();
        }
    }

    pub(crate) fn save(&self) {
        let Some((command, creating)) = self
            .draft
            .borrow()
            .as_ref()
            .map(|d| (d.save_command(), d.mode == EditorMode::Create))
        else {
            return;
        };
        match dispatch_library(&self.project_session, command) {
            Err(why) => self.show_error(why),
            Ok(events) => {
                if creating {
                    self.busy.set(true);
                    self.error.borrow_mut().clear();
                    self.publish();
                } else {
                    self.close();
                }
                self.forward_events(&events);
            }
        }
    }

    /// Close without saving; a tone waiting for names is dropped.
    pub(crate) fn cancel(&self) {
        let command = self
            .draft
            .borrow()
            .as_ref()
            .and_then(|d| d.cancel_command());
        if let Some(command) = command {
            if let Ok(events) = dispatch_library(&self.project_session, command) {
                self.forward_events(&events);
            }
        }
        self.close();
    }

    /// Make a kept version current, then show it.
    pub(crate) fn restore(&self, version: &str) {
        let (Some(plugin_id), Ok(version)) = (self.plugin_id(), version.parse::<u32>()) else {
            return;
        };
        let command = Command::PluginLibrary(PluginLibraryCommand::RestorePluginVersion {
            plugin_id: plugin_id.clone(),
            version,
        });
        match dispatch_library(&self.project_session, command) {
            Err(why) => self.show_error(why),
            Ok(events) => {
                self.reload(&plugin_id);
                self.forward_events(&events);
            }
        }
    }

    /// Build the parameters again; busy until the answer arrives.
    pub(crate) fn redo(&self) {
        let Some(plugin_id) = self.plugin_id() else {
            return;
        };
        self.busy.set(true);
        self.error.borrow_mut().clear();
        self.publish();
        let command =
            Command::PluginLibrary(PluginLibraryCommand::RedoPluginParameters { plugin_id });
        match dispatch_library(&self.project_session, command) {
            Err(why) => {
                self.busy.set(false);
                self.show_error(why);
            }
            Ok(events) => {
                self.on_events(&events);
                self.forward_events(&events);
            }
        }
    }

    /// A drain delivered library events: settle a running create or redo.
    pub(crate) fn on_events(&self, events: &[Event]) {
        if !self.busy.get() {
            return;
        }
        let mode = self.draft.borrow().as_ref().map(|d| d.mode.clone());
        let plugin_id = self.plugin_id();
        for event in events {
            let Event::PluginLibrary(event) = event else {
                continue;
            };
            match (event, &mode) {
                (PluginLibraryEvent::Created { .. }, Some(EditorMode::Create)) => {
                    self.close();
                    return;
                }
                (PluginLibraryEvent::CreateFailed { message }, Some(EditorMode::Create)) => {
                    self.busy.set(false);
                    self.show_error(message.clone());
                }
                (PluginLibraryEvent::Redone { plugin_id: id, .. }, _)
                    if Some(id) == plugin_id.as_ref() =>
                {
                    self.busy.set(false);
                    self.reload(id);
                }
                (
                    PluginLibraryEvent::RedoFailed {
                        plugin_id: id,
                        message,
                    },
                    _,
                ) if Some(id) == plugin_id.as_ref() => {
                    self.busy.set(false);
                    self.show_error(message.clone());
                }
                _ => {}
            }
        }
    }

    /// One tick: collect a running create or redo, and open the first tone
    /// that waits for names when the editor is free.
    pub(crate) fn tick(&self) {
        if self.busy.get() {
            let events = poll_tone3000(&self.project_session);
            self.forward_events(&events);
        }
        if self.draft.borrow().is_some() {
            return;
        }
        let pending =
            tone3000_snapshot(&self.project_session).and_then(|s| s.naming.into_iter().next());
        if let Some(pending) = pending {
            *self.header.borrow_mut() = Header {
                subject: pending.plugin_id.clone(),
                tone3000: true,
            };
            self.open(EditorDraft::naming(&pending));
        }
    }

    #[cfg(test)]
    pub(crate) fn draft(&self) -> Option<EditorDraft> {
        self.draft.borrow().clone()
    }

    #[cfg(test)]
    pub(crate) fn is_busy(&self) -> bool {
        self.busy.get()
    }

    fn plugin_id(&self) -> Option<String> {
        self.draft
            .borrow()
            .as_ref()
            .and_then(|d| d.plugin_id().map(str::to_string))
    }

    fn open(&self, draft: EditorDraft) {
        *self.draft.borrow_mut() = Some(draft);
        self.busy.set(false);
        self.error.borrow_mut().clear();
        self.publish();
        (self.show)();
    }

    fn close(&self) {
        *self.draft.borrow_mut() = None;
        self.busy.set(false);
        self.error.borrow_mut().clear();
        (self.hide)();
    }

    /// Read the plugin's grid again after a restore or a redo.
    fn reload(&self, plugin_id: &str) {
        let fresh = plugin_grid_of(&self.project_session, plugin_id)
            .and_then(Result::ok)
            .and_then(EditorDraft::edit);
        if let Some(fresh) = fresh {
            *self.draft.borrow_mut() = Some(fresh);
        }
        self.publish();
    }

    fn show_error(&self, why: String) {
        *self.error.borrow_mut() = why;
        self.publish();
    }

    fn forward_events(&self, events: &[Event]) {
        if !events.is_empty() {
            (self.forward)(events);
        }
    }

    fn publish(&self) {
        let (Some(w), Some(draft)) = (self.window.upgrade(), self.draft.borrow().clone()) else {
            return;
        };
        let header = self.header.borrow();
        let error = self.error.borrow();
        let chrome = EditorChrome {
            subject: &header.subject,
            tone3000: header.tone3000,
            busy: self.busy.get(),
            error: &error,
        };
        set_editor_view(&PluginEditorBridge::get(&w), &draft, &chrome);
    }
}

#[cfg(test)]
#[path = "plugin_editor_ctx_tests.rs"]
mod tests;
