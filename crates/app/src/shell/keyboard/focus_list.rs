//! Stable keyboard targets and scroll visibility for one collection.

use std::collections::BTreeMap;

use nsh_core::NodeId;
use nsh_core::prelude::*;

#[derive(Clone, Copy)]
pub(crate) struct FocusList {
    refs: Stored<BTreeMap<String, ElementRef>>,
    order: Signal<Vec<String>>,
    last: Stored<Option<(String, usize)>>,
    focused: Stored<Option<ElementRef>>,
    request: Stored<u64>,
    offset: Stored<Option<(NodeId, Point<Points>)>>,
    pub fallback: ElementRef,
    pub scroll: ElementRef,
}

impl FocusList {
    pub(super) fn new() -> Self {
        Self {
            refs: stored(BTreeMap::new()),
            order: signal(Vec::new()),
            last: stored(None),
            focused: stored(None),
            request: stored(0),
            offset: stored(None),
            fallback: element_ref(),
            scroll: element_ref(),
        }
    }

    pub(super) fn register(self, id: &str) -> ElementRef {
        let reference = element_ref();
        self.refs.with(|refs| refs.insert(id.to_owned(), reference));
        let id = id.to_owned();
        cleanup(move || {
            self.refs.try_with(|refs| {
                if refs.get(&id) == Some(&reference) {
                    refs.remove(&id);
                }
            });
        });
        reference
    }

    pub(super) fn track(self, id: &str, reference: ElementRef) {
        let id = id.to_owned();
        reference.on_focus_in(move || {
            self.focused.replace(Some(reference));
            self.note_focus(&id);
        });
        reference.on_focus_out(move || {
            if self.focused.get() == Some(reference) {
                self.focused.replace(None);
            }
        });
    }

    pub(crate) fn set_order(self, order: Vec<String>, restore: bool) {
        let focused = self.focused.get();
        self.last.with(|last| {
            if let Some((id, index)) = last
                && let Some(current) = order.iter().position(|key| key == id)
            {
                *index = current;
            }
        });
        self.order.set(order);
        if restore && let Some(element) = focused {
            after_answer(move || {
                if element.node().is_none()
                    && self.focused.get().is_none_or(|current| current == element)
                {
                    self.focus_current(None);
                }
            });
        }
    }

    pub(crate) fn on_scroll(self, event: &ScrollEvent) {
        self.offset
            .replace(self.scroll.node().map(|node| (node, event.offset)));
    }

    pub(super) fn contains(self, id: &str) -> bool {
        self.order.get_untracked().iter().any(|key| key == id)
    }

    fn note_focus(self, id: &str) {
        if let Some(index) = self.order.get_untracked().iter().position(|key| key == id) {
            self.last.replace(Some((id.to_owned(), index)));
        }
    }

    pub(super) fn current_key(self) -> Option<String> {
        self.last.get().map(|(id, _)| id)
    }

    pub(crate) fn has_focus(self) -> bool {
        self.focused
            .get()
            .is_some_and(|element| element.node().is_some())
    }

    pub(crate) fn focus_current(self, preferred: Option<&str>) {
        let order = self.order.get_untracked();
        let last = self.last.get();
        let preferred = preferred.filter(|id| order.iter().any(|key| key == id));
        let id = last
            .as_ref()
            .and_then(|(id, _)| order.iter().find(|key| *key == id))
            .map(String::as_str)
            .or(preferred)
            .or_else(|| {
                order
                    .get(last.map_or(0, |(_, i)| i.min(order.len().saturating_sub(1))))
                    .map(String::as_str)
            });
        if let Some(id) = id {
            self.focus(id);
        } else {
            self.fallback.focus();
        }
    }

    pub(crate) fn focus_first(self) {
        if self.order.get_untracked().is_empty() {
            self.fallback.focus();
        } else {
            self.edge(false);
        }
    }

    pub(crate) fn focus(self, id: &str) {
        if let Some(reference) = self.refs.with(|refs| refs.get(id).copied())
            && reference.node().is_some()
        {
            self.note_focus(id);
            reference.focus();
            self.reveal(reference);
        }
    }

    pub(super) fn step(self, id: &str, delta: isize) {
        let order = self.order.get_untracked();
        if let Some(index) = order.iter().position(|key| key == id) {
            let next = index.saturating_add_signed(delta).min(order.len() - 1);
            if next != index {
                self.focus(&order[next]);
            }
        }
    }

    pub(super) fn edge(self, end: bool) {
        let order = self.order.get_untracked();
        if let Some(id) = if end { order.last() } else { order.first() } {
            self.focus(id);
        }
    }

    fn reveal(self, target: ElementRef) {
        let Some(node) = self.scroll.node() else {
            return;
        };
        let request = self.request.with(|n| {
            *n = n.wrapping_add(1);
            *n
        });
        let observed = self.offset.get();
        let offset = observed
            .filter(|(scroller, _)| *scroller == node)
            .map_or(point(px(0.0), px(0.0)), |(_, offset)| offset);
        let viewport = self.scroll.measure();
        let target = target.measure();
        spawn(async move {
            let (Some(viewport), Some(target)) = (viewport.await, target.await) else {
                return;
            };
            if self.request.get() != request
                || self.scroll.node() != Some(node)
                || self.offset.get() != observed
            {
                return;
            }
            let top = target.page.y.0 - viewport.page.y.0;
            let bottom = top + target.frame.size.height.0 - viewport.frame.size.height.0;
            let delta = if top < 0.0 || target.frame.size.height > viewport.frame.size.height {
                top
            } else {
                bottom.max(0.0)
            };
            if delta.abs() > 0.5 {
                self.scroll
                    .scroll_to(point(offset.x, px((offset.y.0 + delta).max(0.0))), false);
            }
        });
    }
}
