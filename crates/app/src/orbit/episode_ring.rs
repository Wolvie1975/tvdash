//! Episode ring pagination and node sizing (§6.3).

pub const PAGE: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RingPage {
    pub first: usize,
    pub count: usize,
    pub page: usize,
    pub pages: usize,
}

pub fn page_for(total: usize, focused: usize) -> RingPage {
    let pages = total.div_ceil(PAGE).max(1);
    let page = (focused / PAGE).min(pages - 1);
    let first = page * PAGE;
    let count = total.saturating_sub(first).min(PAGE);
    RingPage { first, count, page, pages }
}

pub fn node_size(count: usize, next: bool) -> f32 {
    match (count > 12, next) {
        (false, false) => 76.0,
        (false, true) => 132.0,
        (true, false) => 54.0,
        (true, true) => 112.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_episodes_single_page() {
        let p = page_for(8, 0);
        assert_eq!(p, RingPage { first: 0, count: 8, page: 0, pages: 1 });
        assert_eq!(node_size(p.count, false), 76.0);
        assert_eq!(node_size(p.count, true), 132.0);
    }

    #[test]
    fn twenty_two_episodes_dense_single_page() {
        let p = page_for(22, 5);
        assert_eq!(p, RingPage { first: 0, count: 22, page: 0, pages: 1 });
        assert_eq!(node_size(p.count, false), 54.0);
        assert_eq!(node_size(p.count, true), 112.0);
    }

    #[test]
    fn fifty_episodes_pages_of_24() {
        let p2 = page_for(50, 30);
        assert_eq!(p2, RingPage { first: 24, count: 24, page: 1, pages: 3 });

        let p3 = page_for(50, 49);
        assert_eq!(p3, RingPage { first: 48, count: 2, page: 2, pages: 3 });
    }
}
