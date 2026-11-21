//! **PLATFORM GLUE (tvOS text entry, TVOS.md)** — the fields that take typed text (the profile name, the
//! front end's input boxes, the input dialogs, the replay-save name) get their text from the platform's
//! on-screen keyboard where there is no keyboard: the app shell shows a native prompt and hands the text
//! back, which goes in through the **same** paths the keys take (`type_char` / backspace / Return).

#![cfg_attr(not(target_os = "tvos"), allow(dead_code))]

use crate::*;

/// What currently wants typed text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextTarget {
    /// the replay-save dialog's name (`HReplaySave`)
    ReplaySave,
    /// an input dialog's field (`fe.dialog`)
    Dialog,
    /// a front-end input box (layout element index)
    Input(usize),
}

/// A request for the shell's prompt.
#[derive(Debug, Clone)]
pub(crate) struct TextRequest {
    pub(crate) target: TextTarget,
    pub(crate) title: String,
    pub(crate) current: String,
    pub(crate) max_len: usize,
    /// a chat box: always focused, so it's only prompted for on request (pad Y), never automatically
    pub(crate) chat: bool,
}

impl App {
    /// The field that takes typed text right now, if any.
    pub(crate) fn text_request(&self) -> Option<TextRequest> {
        if self.replay_save.up && !self.replay_save.overwrite && !self.paused {
            let current = if self.replay_save.name.is_empty() { self.default_replay_name() } else { self.replay_save.name.clone() };
            return Some(TextRequest { target: TextTarget::ReplaySave, title: "Replay name".into(), current, max_len: 11, chat: false });
        }
        if let Some((d, _)) = self.fe.dialog.as_ref()
            && d.up
            && let Some(f) = d.field.as_ref()
        {
            return Some(TextRequest { target: TextTarget::Dialog, title: d.text.clone(), current: f.text.clone(), max_len: f.max, chat: false });
        }
        if self.fe.dialog.is_none()
            && let Some(l) = self.fe.layout_preview.as_ref()
            && let Some(i) = l.focused_input()
        {
            let (current, accessor) = l.edit_state(i);
            let chat = l.class_of(i).as_deref() == Some("NFS5.CHATINPUTBOX");
            let title = match accessor.as_deref() {
                Some("UserList.NameInput") => "Player name".to_string(),
                Some(a) if a.eq_ignore_ascii_case("Comms.IP") || a.contains("IP") => "Address (IP or host name)".to_string(),
                Some(a) if a.contains("Chat") => "Message".to_string(),
                Some(a) if a.to_ascii_uppercase().contains("PASSWORD") => "Password".to_string(),
                Some(a) => a.rsplit('.').next().unwrap_or(a).to_string(),
                None => "Text".to_string(),
            };
            let max_len = accessor.as_deref().and_then(nfs_game::layout::LayoutScreen::input_max_len).unwrap_or(63);
            return Some(TextRequest { target: TextTarget::Input(i), title, current, max_len, chat });
        }
        None
    }

    /// Replace the target's text with `text` and commit it as Return does. Returns whether it went in.
    pub(crate) fn inject_text(&mut self, target: TextTarget, text: &str) -> bool {
        // the target must still be the one the prompt was for
        if self.text_request().map(|r| r.target) != Some(target) {
            tracing::info!(?target, "text entry: the field went away before the text arrived");
            return false;
        }
        match target {
            TextTarget::ReplaySave => {
                self.replay_save.name.clear();
                for c in text.chars() {
                    self.replay_save.type_char(c);
                }
                // Return = the dialog's first button, "save" (0x42E375's path, as the pad's A)
                self.replay_save_click(0);
            }
            TextTarget::Dialog => {
                let Some((d, _)) = self.fe.dialog.as_mut() else { return false };
                while d.field.as_ref().is_some_and(|f| !f.text.is_empty()) {
                    d.backspace();
                }
                for c in text.chars() {
                    d.type_char(c);
                }
                d.enter();
                self.fe_sound(9);
            }
            TextTarget::Input(i) => {
                if let Some(l) = self.fe.layout_preview.as_mut() {
                    for _ in 0..256 {
                        if !l.backspace(i) {
                            break;
                        }
                    }
                }
                for c in text.chars() {
                    self.fe_input_key(i, c);
                }
                self.bind_player_buttons();
                self.fe_input_return(i);
            }
        }
        true
    }

    /// Return on a focused input box (0x4CCC20's `0x0D`, or the chat / char-vector boxes' own slot 4). Returns
    /// whether the key was taken — shared by the keyboard and the text prompt.
    pub(crate) fn fe_input_return(&mut self, i: usize) -> bool {
        // the chat box's own Return, 0x4C3310 (`netplay::NetPlay::say`)
        if let Some(text) = self.fe.layout_preview.as_mut().and_then(|l| l.take_chat(i)) {
            let empty = self.fe.strings.get(287).unwrap_or_default().to_string();
            if let Some(n) = self.net.session.as_mut() {
                n.say(&text, &empty);
            }
            self.fe_sound(9);
            self.bind_player_buttons();
            return true;
        }
        // `NFS5.CHARVECTACCESSORINPUTBOX`'s own Return, slot 4 0x4C2120 (see the key handler)
        if self.fe.layout_preview.as_ref().and_then(|l| l.class_of(i)).as_deref() == Some("NFS5.CHARVECTACCESSORINPUTBOX") {
            self.fe_charvect_return(i);
            return true;
        }
        if let Some(l) = self.fe.layout_preview.as_mut()
            && let Some((name, text)) = l.commit_input(i)
        {
            tracing::info!(accessor = %name, value = %text, "input box committed");
            self.fe_sound(9);
            self.bind_player_buttons();
            return true;
        }
        false
    }
}
