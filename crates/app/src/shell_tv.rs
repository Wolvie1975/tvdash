//! TV-only shell state: tabs, page stack, Back navigation, and per-page focus memory (§4.2).

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tab {
    Home = 0,
    Library = 1,
    Collections = 2,
    Sports = 3,
    Settings = 4,
}

impl Tab {
    pub fn from_index(idx: usize) -> Option<Self> {
        match idx {
            0 => Some(Self::Home),
            1 => Some(Self::Library),
            2 => Some(Self::Collections),
            3 => Some(Self::Sports),
            4 => Some(Self::Settings),
            _ => None,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Home => Self::Library,
            Self::Library => Self::Collections,
            Self::Collections => Self::Sports,
            Self::Sports => Self::Settings,
            Self::Settings => Self::Settings,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Home => Self::Home,
            Self::Library => Self::Home,
            Self::Collections => Self::Library,
            Self::Sports => Self::Collections,
            Self::Settings => Self::Sports,
        }
    }

    pub fn page(self) -> Page {
        match self {
            Self::Home => Page::Home,
            Self::Library => Page::Library,
            Self::Collections => Page::Collections,
            Self::Sports => Page::Sports,
            Self::Settings => Page::Settings,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Page {
    Home,
    Library,
    Show(String),
    Collection(String),
    Collections,
    Sports,
    Search,
    Settings,
}

#[derive(Debug, Clone)]
pub struct ShellState {
    pub tab: Tab,
    pub stack: Vec<Page>,
    pub bar_focused: bool,
    pub focused_tile: usize,
    pub last_focus: HashMap<Page, usize>,
}

impl Default for ShellState {
    fn default() -> Self {
        Self {
            tab: Tab::Home,
            stack: vec![Page::Home],
            bar_focused: false,
            focused_tile: 1, // Hero tile on Home
            last_focus: HashMap::new(),
        }
    }
}

impl ShellState {
    pub fn current(&self) -> &Page {
        self.stack.last().unwrap_or(&Page::Home)
    }

    pub fn save_focus(&mut self) {
        let page = self.current().clone();
        self.last_focus.insert(page, self.focused_tile);
    }

    /// Switch top-level tab immediately, restoring the last focused tile on that page.
    pub fn switch_tab(&mut self, tab: Tab) {
        self.save_focus();
        self.tab = tab;
        let page = tab.page();
        self.focused_tile = self.last_focus.get(&page).copied().unwrap_or(0);
        self.stack = vec![page];
    }

    /// Push a detail or Search page onto the stack.
    pub fn push(&mut self, page: Page) {
        self.save_focus();
        self.bar_focused = false;
        self.focused_tile = self.last_focus.get(&page).copied().unwrap_or(0);
        self.stack.push(page);
    }

    /// Back: pop detail page → go to Home tab → (on Home) no-op.
    /// Returns `true` if navigation state changed.
    pub fn back(&mut self) -> bool {
        if self.stack.len() > 1 {
            self.save_focus();
            self.stack.pop();
            self.bar_focused = false;
            self.focused_tile = self.last_focus.get(self.current()).copied().unwrap_or(0);
            return true;
        }
        if self.tab != Tab::Home {
            self.switch_tab(Tab::Home);
            self.bar_focused = false;
            return true;
        }
        if self.bar_focused {
            self.bar_focused = false;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_back_restores_focus_and_returns_home() {
        let mut s = ShellState::default();
        s.focused_tile = 2; // Search icon on Home
        s.push(Page::Search);
        assert_eq!(s.current(), &Page::Search);
        assert_eq!(s.focused_tile, 0);

        assert!(s.back());
        assert_eq!(s.current(), &Page::Home);
        assert_eq!(s.focused_tile, 2);

        // Switching to Library then pressing Back goes to Home tab
        s.switch_tab(Tab::Library);
        assert_eq!(s.current(), &Page::Library);
        assert!(s.back());
        assert_eq!(s.current(), &Page::Home);
        assert_eq!(s.tab, Tab::Home);

        // On Home with bar unfocused, Back is a no-op
        assert!(!s.back());
    }
}
