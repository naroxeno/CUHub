//! Scoped keyboard navigation for the sidebar, local list and task drawer.

use nsh_core::keymap::{Keystroke, Modifiers, When};
use nsh_core::prelude::*;

mod focus_list;

use self::focus_list::FocusList;
use super::nav;
use crate::app_state::Ui;
use crate::components::{pix, scaled};
use crate::palette::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Area {
    Sidebar,
    Local,
    TaskEntry,
    Tasks,
}

#[derive(Clone, Copy)]
pub(crate) struct Keyboard {
    pub sidebar: FocusList,
    pub local: FocusList,
    pub tasks: FocusList,
    pub task_entry: ElementRef,
    area: Signal<Option<Area>>,
    prefix: Signal<bool>,
    menu: Signal<Option<String>>,
    blocked: Memo<bool>,
    page: Signal<&'static str>,
    task_open: Signal<bool>,
    task_target: Signal<Option<&'static str>>,
    task_origin: Stored<Option<Area>>,
    task_fallback: Stored<Option<String>>,
    pub(crate) bulk_delete: Stored<bool>,
}

impl Keyboard {
    pub(crate) fn new(ui: &Ui) -> Self {
        let state = ui.clone();
        let keys = Self {
            sidebar: FocusList::new(),
            local: FocusList::new(),
            tasks: FocusList::new(),
            task_entry: element_ref(),
            area: signal(None),
            prefix: signal(false),
            menu: signal(None),
            blocked: computed(move || {
                state.setup.open.get()
                    || state.dev.get()
                    || state.bepinex_confirm.get()
                    || state.delete_prompt.get().is_some()
            }),
            page: ui.page,
            task_open: ui.task_open,
            task_target: ui.task_focus,
            task_origin: stored(None),
            task_fallback: stored(None),
            bulk_delete: stored(false),
        };
        effect(move || {
            if keys.blocked.get() || keys.task_open.get() || keys.menu.get().is_some() {
                keys.prefix.set(false);
                return;
            }
            for (key, backwards) in [("f6", false), ("shift+f6", true)] {
                on_key_with(
                    key,
                    KeyOptions {
                        when: When::NotEditing,
                        ..Default::default()
                    },
                    move |_| keys.cycle(backwards),
                );
            }
        });
        keys
    }

    pub(crate) fn get() -> Self {
        use_context().expect("shell keyboard context")
    }

    pub(crate) fn active(self, area: Area) -> bool {
        self.area.get_untracked() == Some(area)
    }

    pub(crate) fn track_area(self, element: ElementRef, area: Area) {
        element.on_focus_in(move || {
            self.area.set(Some(area));
            self.prefix.set(false);
        });
        element.on_focus_out(move || {
            if self.area.get_untracked() == Some(area) {
                self.area.set(None);
                self.prefix.set(false);
            }
        });
    }

    pub(crate) fn menu_open(self, id: &str, open: bool) {
        if open {
            self.menu.try_set(Some(id.to_owned()));
        } else if untracked(|| self.menu.try_get().flatten()).as_deref() == Some(id) {
            self.menu.try_set(None);
        }
    }

    fn cycle(self, backwards: bool) {
        if self.blocked.get_untracked()
            || self.task_open.get_untracked()
            || self.menu.get_untracked().is_some()
        {
            return;
        }
        let areas: &[Area] = if self.page.get_untracked() == nav::LOCAL {
            &[Area::Sidebar, Area::Local, Area::TaskEntry]
        } else {
            &[Area::Sidebar, Area::TaskEntry]
        };
        let current = self.area.get_untracked();
        let next = areas.iter().position(|area| Some(*area) == current);
        let index = match (next, backwards) {
            (Some(index), false) => (index + 1) % areas.len(),
            (Some(index), true) => (index + areas.len() - 1) % areas.len(),
            (None, false) => 0,
            (None, true) => areas.len() - 1,
        };
        match areas[index] {
            Area::Sidebar => self.focus_sidebar(),
            Area::Local => self.local.focus_current(None),
            Area::TaskEntry => self.task_entry.focus(),
            Area::Tasks => {}
        }
    }

    fn focus_sidebar(self) {
        let page = self.page.get_untracked();
        let preferred = if nav::in_group(page, nav::GROUP_MODS) && !self.sidebar.contains(page) {
            nav::GROUP_MODS
        } else {
            page
        };
        self.sidebar.focus_current(Some(preferred));
    }

    pub(crate) fn open_tasks(self, source: Option<Area>) {
        self.task_origin.replace(source);
        self.task_fallback.replace(match source {
            Some(Area::Sidebar) => self.sidebar.current_key(),
            Some(Area::Local) => self.local.current_key(),
            _ => None,
        });
        self.task_target.set(None);
        self.task_open.set(true);
    }

    pub(crate) fn restore_after_tasks(self) {
        let Some(origin) = self.task_origin.replace(None) else {
            return;
        };
        let target = self.task_fallback.replace(None);
        spawn(async move {
            next_frame().await;
            if self.task_open.get_untracked() || self.blocked.get_untracked() {
                return;
            }
            match origin {
                Area::Sidebar => self.sidebar.focus_current(target.as_deref()),
                Area::Local if self.page.get_untracked() == nav::LOCAL => {
                    self.local.focus_current(target.as_deref());
                }
                Area::Local => self.focus_sidebar(),
                Area::TaskEntry => self.task_entry.focus(),
                Area::Tasks => {}
            }
        });
    }

    pub(crate) fn can_restore_local(self) -> bool {
        self.page.get_untracked() == nav::LOCAL
            && !self.blocked.get_untracked()
            && !self.task_open.get_untracked()
            && self.menu.get_untracked().is_none()
            && self.area.get_untracked().is_none()
    }

    pub(crate) fn hint(self, zoom: Signal<u8>, drawer: bool) -> impl IntoElement {
        when(
            move || self.prefix.get() && !self.blocked.get() && self.task_open.get() == drawer,
            move || {
                view()
                    .absolute()
                    .pointer_events(PointerEvents::None)
                    .bottom(scaled(&zoom, 10.0))
                    .left(scaled(&zoom, 12.0))
                    .px(scaled(&zoom, 7.0))
                    .py(scaled(&zoom, 4.0))
                    .bg(theme_color(SURFACE_RAISED))
                    .border(scaled(&zoom, 1.0))
                    .border_color(theme_color(BORDER))
                    .child(pix(
                        move || {
                            t!(if drawer {
                                "keyboard.goto_tasks"
                            } else {
                                "keyboard.goto"
                            })
                        },
                        11.0,
                        TEXT,
                        &zoom,
                    ))
            },
        )
    }

    pub(crate) fn item(self, view: View, item: FocusItem) -> View {
        let FocusItem {
            area,
            id,
            extra,
            action,
            back,
        } = item;
        let list = match area {
            Area::Sidebar => self.sidebar,
            Area::Local => self.local,
            Area::Tasks => self.tasks,
            Area::TaskEntry => unreachable!(),
        };
        let reference = list.register(&id);
        let exact = signal(false);
        let mut keys = vec!["j", "k", "up", "down", "home", "end"];
        keys.push("g");
        keys.extend(extra);
        let regular = captures(&keys);
        let goto = if area == Area::Tasks {
            captures(&["g", "e", "escape", "enter", "space"])
        } else {
            captures(&["g", "e", "h", "j", "l", "escape", "enter", "space"])
        };
        let back_key = captures(&["h"]);
        list.track(&id, reference);
        view.element_ref(&reference)
            .focusable(true)
            .on_focus(move || exact.set(true))
            .on_blur(move || {
                exact.set(false);
                self.prefix.set(false);
            })
            .capture_keys(move || {
                if self.blocked.get()
                    || self.menu.get().is_some()
                    || (area != Area::Tasks && self.task_open.get())
                {
                    KeyCapture::None
                } else if exact.get() {
                    if self.prefix.get() {
                        goto.clone()
                    } else {
                        regular.clone()
                    }
                } else if back.is_some_and(|active| active.get()) {
                    back_key.clone()
                } else {
                    KeyCapture::None
                }
            })
            .on_key_down(move |event: &KeyEvent| {
                if self.blocked.get_untracked()
                    || self.menu.get_untracked().is_some()
                    || (area != Area::Tasks && self.task_open.get_untracked())
                    || event.modifiers != Modifiers::default()
                {
                    return;
                }
                let key = event.key.as_str();
                if !exact.get_untracked() {
                    if event.default_prevented
                        && !event.repeat
                        && key == "h"
                        && back.is_some_and(|active| active.get_untracked())
                    {
                        list.focus(&id);
                    }
                    return;
                }
                if self.prefix.get_untracked() {
                    if !event.repeat {
                        self.prefix.set(false);
                        self.goto(area, list, key);
                    }
                    return;
                }
                if !event.default_prevented {
                    return;
                }
                match key {
                    "j" | "ArrowDown" => list.step(&id, 1),
                    "k" | "ArrowUp" => list.step(&id, -1),
                    "Home" => list.edge(false),
                    "End" => list.edge(true),
                    "g" if !event.repeat => self.prefix.set(true),
                    key if !event.repeat => action(key),
                    _ => {}
                }
            })
    }

    fn goto(self, area: Area, list: FocusList, key: &str) {
        match key {
            "g" => list.edge(false),
            "e" => list.edge(true),
            _ if area == Area::Tasks => {}
            "h" => self.focus_sidebar(),
            "j" if self.page.get_untracked() == nav::LOCAL => self.local.focus_current(None),
            "j" => {
                self.page.set(nav::LOCAL);
                after_answer(move || self.local.focus_current(None));
            }
            "l" => self.open_tasks(Some(area)),
            _ => {}
        }
    }
}

pub(crate) struct FocusItem {
    pub area: Area,
    pub id: String,
    pub extra: &'static [&'static str],
    pub action: Box<dyn Fn(&str)>,
    pub back: Option<Signal<bool>>,
}

fn captures(keys: &[&str]) -> KeyCapture {
    KeyCapture::keys(
        keys.iter()
            .map(|key| Keystroke::parse_with(key, false).expect("valid navigation key")),
    )
}
