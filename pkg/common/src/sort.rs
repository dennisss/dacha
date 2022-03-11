use core::cmp::Ordering;

pub fn bubble_sort_by<T, F: FnMut(&T, &T) -> Ordering>(items: &mut [T], mut f: F) {
    if items.len() == 0 {
        return;
    }

    // Largest index at which a swap occured in the last index.
    // All items after this index are sorted.
    let mut n = items.len() - 1;

    while n > 0 {
        // Next value of 'n'. If no swaps occur, all values are sorted.
        let mut next_n = 0;

        for i in 0..n {
            if f(&items[i], &items[i + 1]).is_gt() {
                items.swap(i, i + 1);
                next_n = i;
            }
        }

        n = next_n;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn bubble_sort_by_test() {
        let mut items = [1, 2, 3, 4, 5];
        bubble_sort_by(&mut items, |a, b| a.cmp(b));
        assert_eq!(&items, &[1, 2, 3, 4, 5]);

        let mut items = [5, 4, 3, 2, 1];
        bubble_sort_by(&mut items, |a, b| a.cmp(b));
        assert_eq!(&items, &[1, 2, 3, 4, 5]);

        let mut items = [10];
        bubble_sort_by(&mut items, |a, b| a.cmp(b));
        assert_eq!(&items, &[10]);

        let mut items = [];
        bubble_sort_by(&mut items, |a: &u32, b| a.cmp(b));
        assert_eq!(&items, &[]);

        let mut items = [16, 15];
        bubble_sort_by(&mut items, |a: &u32, b| a.cmp(b));
        assert_eq!(&items, &[15, 16]);

        let mut items = [5, 4, 10, 10, 3, 2, 1];
        bubble_sort_by(&mut items, |a: &u32, b| a.cmp(b));
        assert_eq!(&items, &[1, 2, 3, 4, 5, 10, 10]);
    }
}
