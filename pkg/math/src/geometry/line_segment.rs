use alloc::vec::Vec;
use core::cmp::Ordering;

use common::tree::avl::AVLTree;
use common::tree::binary_heap::BinaryHeap;
use common::InRange;

use crate::geometry::line::Line2f;
use crate::matrix::{Matrix2f, Vector2f};

/// Bounded line segment defined by two endpoints which are connected.
/// The two endpoints are inclusive (considered to be part of the segment).
#[derive(Debug, PartialEq)]
pub struct LineSegment2f {
    pub start: Vector2f,
    pub end: Vector2f,
}

impl LineSegment2f {
    pub fn intersect(&self, other: &Self) -> Option<Vector2f> {
        // TODO: If either endpoint of either line is on the other line, return exactly
        // that point rather than the calculated intersection.

        let current_line = Line2f::from_points(&self.start, &self.end);
        let other_line = Line2f::from_points(&other.start, &other.end);

        let point = match current_line.intersect(&other_line) {
            Some(p) => p,
            None => {
                return None;
            }
        };

        let on_segment = |segment: &LineSegment2f, line: &Line2f, point: &Vector2f| -> bool {
            if intersections::compare_points(point, &segment.start).is_eq()
                || intersections::compare_points(point, &segment.end).is_eq()
            {
                return true;
            }

            let t = (point - &current_line.base).norm() / line.dir.norm();

            t.in_range(0., 1.)
        };

        if !on_segment(self, &current_line, &point) || !on_segment(other, &other_line, &point) {
            return None;
        }

        Some(point)
    }

    /// Finds all intersections between a set of line segments.
    ///
    /// Internally uses the Bentley-Ottmann algorithm.
    ///
    /// TODO: what should this return if there are overlapping segments?
    ///
    /// TODO: For each intersection, we also want to know which segments where
    /// involved (one or more segment indices)
    pub fn intersections<'a>(segments: &'a [Self]) -> Vec<Intersection2f<'a>> {
        use self::intersections::*;

        let mut output = vec![];

        // Ordered set of points which we want to visit next. We sweep a line from low
        // to high y values.
        let mut event_queue = BinaryHeap::<Event>::default();
        for segment in segments {
            let (upper, lower) = upper_lower_endpoints(segment);

            // TODO: What should we do if the segment is a point (upper ~= lower) as that
            // will mean that the upper and lower segments will be processed in the same
            // event.

            event_queue.insert(Event {
                point: upper,
                segment: Some(segment),
            });

            // NOTE: If upper ~= lower, the algorithm still works reasonably correctly as we
            // never insert segments in into the sweep_status when the current event point
            // is equal to the lower point.
            event_queue.insert(Event {
                point: lower,
                segment: None,
            });
        }

        // Ordered list of line segments which intersect with the last sweep line (at
        // the last event).
        let mut sweep_status = AVLTree::<&LineSegment2f>::new();

        // Last point which was used for inserting things into sweep_status.
        let mut last_sweep_point = Vector2f::zero();

        while let Some(first_event) = event_queue.extract_min() {
            let point = first_event.point;

            // List of all segments whose upper endpoint is at this event point (this are
            // all not yet in the sweep_status and just in consecutive equal event points).
            let mut upper_segments = vec![];
            {
                if let Some(segment) = first_event.segment {
                    upper_segments.push(segment);
                }
                while let Some(event) = event_queue.peek_min() {
                    if compare_points(&point, &event.point).is_eq() {
                        if let Some(segment) = event.segment.clone() {
                            upper_segments.push(segment);
                        }

                        event_queue.extract_min();
                    } else {
                        break;
                    }
                }
            }

            let mut existing_segments = vec![];
            {
                let mut iter = sweep_status.lower_bound_by(&point, &|segment, point| {
                    sweep_line_x(segment, point)
                        .partial_cmp(&point.x())
                        .unwrap_or(Ordering::Equal)
                });

                while let Some(segment) = iter.next().cloned() {
                    if (sweep_line_x(segment, &point) - point.x()).abs() < THRESHOLD {
                        existing_segments.push(segment);
                    } else {
                        break;
                    }
                }
            }

            // Report an intersection
            if upper_segments.len() + existing_segments.len() > 1 {
                let mut segments = vec![];
                segments.extend_from_slice(&upper_segments);
                segments.extend_from_slice(&existing_segments);

                output.push(Intersection2f {
                    point: point.clone(),
                    segments,
                });
            }

            // Remove all segments that we touched (will be re-inserted in the
            // next step).
            // NOTE: We use the last sweep point in the comparator to ensure search
            // stability.
            for segment in existing_segments.iter().cloned() {
                sweep_status.remove_by(&segment, &|a, b| {
                    compare_segments_at_sweep_line(a, b, &last_sweep_point)
                });
            }

            // Of the segments we are about to insert, this tracks the left most and right
            // most ones.
            let mut first_last_segment = None;

            // (Re-)Insert all segments which had an upper endpoint as the
            // current segment or was already in the sweep status and has an intersection in
            for segment in upper_segments
                .iter()
                .cloned()
                .chain(existing_segments.iter().cloned())
            {
                // Don't insert any segments with the lower endpoint equal to the current event
                // point (this is how segments eventually get removed from the status).
                let (_, lower) = upper_lower_endpoints(segment);
                if compare_points(&point, &lower).is_eq() {
                    continue;
                }

                sweep_status.insert_by(segment, &|a, b| {
                    compare_segments_at_sweep_line(a, b, &point)
                });

                first_last_segment = Some(match first_last_segment.take() {
                    Some((mut first, mut last)) => {
                        if compare_segments_at_sweep_line(segment, first, &point).is_lt() {
                            first = segment;
                        }
                        if compare_segments_at_sweep_line(segment, last, &point).is_gt() {
                            last = segment;
                        }

                        (first, last)
                    }
                    None => (segment, segment),
                });
            }

            if let Some((first, last)) = first_last_segment {
                // NOTE: unwrap() should never fail if all the logic is correct as we just
                // inserted these
                let mut first_iter = sweep_status
                    .find(first, &|a, b| compare_segments_at_sweep_line(a, b, &point))
                    .unwrap();
                let mut last_iter = sweep_status
                    .find(last, &|a, b| compare_segments_at_sweep_line(a, b, &point))
                    .unwrap();

                // TODO: Verify that compare_segments_at_sweep_line is
                // sufficienctly robust that segments that aren't exactly equal
                // don't get compared as Ordering::Equal. Otherwise we will need
                // to continue advancing the iterators forward/reverse to skip
                // over any other equal segments.

                first_iter.prev(); // Skip the 'first'
                let first2 = first_iter.peek().cloned();

                last_iter.next(); // Skip over 'last'
                let last2 = last_iter.peek().cloned();

                if let Some(first_neighbor) = first2 {
                    if let Some(next_point) = find_intersection_event(first, first_neighbor, &point)
                    {
                        event_queue.insert(Event {
                            point: next_point,
                            segment: None,
                        });
                    }
                }

                if let Some(last_neighbor) = last2 {
                    if let Some(next_point) = find_intersection_event(last, last_neighbor, &point) {
                        event_queue.insert(Event {
                            point: next_point,
                            segment: None,
                        });
                    }
                }

                //
            } else {
                let mut iter = sweep_status.lower_bound_by(&point, &|segment, point| {
                    sweep_line_x(segment, point)
                        .partial_cmp(&point.x())
                        .unwrap_or(Ordering::Equal)
                });

                let p1 = iter.prev().cloned();
                let p2 = iter.peek().cloned();

                if p1.is_some() && p2.is_some() {
                    if let Some(next_point) =
                        find_intersection_event(p1.unwrap(), p2.unwrap(), &point)
                    {
                        event_queue.insert(Event {
                            point: next_point,
                            segment: None,
                        });
                    }
                }
            }

            last_sweep_point = point;
        }

        /*
        Event queue:

        // This could be a heap given
        BTreeSet<Vector2f, Vec<usize>>
        - Key: Event Point: Sort by y. If y is same, sort by x.
        - Value: List of segments with upper endpoint each to 'p'

        Status Queue (T)
        - Stores line segments basically keyed by their intersection point with the sweep line.
            - Line segments will only swap places if they intersect.

        - I don't want to store the intersection points in the status queue (rather store LineSegment2f)

        - Challenge is that the sorting function will change over time.

        - We could use a BTreeMap but need to store a RefCell<> reference containing the current sweep line (lot's of memory?)

        - Issue is that BTreeMap doesn't define a good behavior for what happens if the comparison method changes in an unexpected way.

        - Main question is: What happens if two elements go from being < to be == (at the intersection).
            - Can't use exact comparison to determine if a segment intersects it (equally challenging for comparing to something that is an endpoint of one line)
        */

        /*
        Sorting two lines at a sweep line p:
        - If both

        */

        output
    }
}

mod intersections {

    use crate::geometry::line::Line2f;

    use super::*;

    pub const THRESHOLD: f32 = 1e-6;

    pub fn upper_lower_endpoints(segment: &LineSegment2f) -> (Vector2f, Vector2f) {
        let mut upper_point = segment.start.clone();
        let mut lower_point = segment.end.clone();
        // TODO: Use exact comparison for this?
        if compare_points(&upper_point, &lower_point).is_gt() {
            core::mem::swap(&mut upper_point, &mut lower_point);
        }

        (upper_point, lower_point)
    }

    // TODO: Verify never called with an empty or horizontal point.
    pub fn sweep_line_x(segment: &LineSegment2f, point: &Vector2f) -> f32 {
        if (segment.end.y() - segment.start.y()).abs() < THRESHOLD {
            let min_x = segment.start.x().min(segment.end.x());
            let max_y = segment.start.x().max(segment.end.x());

            return point.x().min(max_y).max(min_x);
        }

        let t = (point.y() - segment.start.y()) / (segment.end.y() - segment.start.y());
        t * (segment.end.x() - segment.start.x()) + segment.start.x()
    }

    pub fn compare_points(a: &Vector2f, b: &Vector2f) -> Ordering {
        if (a.y() - b.y()).abs() <= THRESHOLD {
            if (a.x() - b.x()).abs() <= THRESHOLD {
                Ordering::Equal
            } else {
                a.x().partial_cmp(&b.x()).unwrap_or(Ordering::Equal)
            }
        } else {
            a.y().partial_cmp(&b.y()).unwrap_or(Ordering::Equal)
        }
    }

    pub fn find_intersection_event(
        a: &LineSegment2f,
        b: &LineSegment2f,
        point: &Vector2f,
    ) -> Option<Vector2f> {
        let intersection = match a.intersect(b) {
            Some(p) => p,
            None => return None,
        };

        if (intersection.y() - point.y()).abs() < THRESHOLD {
            // On sweep line.

            // TODO: Use threshold comparison?
            if intersection.x() <= point.x() {
                return None;
            }
        } else if intersection.y() < point.y() {
            return None;
        }

        Some(intersection)
    }

    // TODO: Ideally this would only return
    pub fn compare_segments_at_sweep_line(
        a: &LineSegment2f,
        b: &LineSegment2f,
        point: &Vector2f,
    ) -> Ordering {
        let a_x = sweep_line_x(a, point);
        let b_x = sweep_line_x(b, point);

        // When both segments are intersecting at the sweep line, we must sort the
        // segments based on their values immediately below the sweep line.
        //
        // To do this we compare the x value of their direction vectors to tell which
        // will move left or right after crossing the intersection (heading towards
        // increasing y values).
        if (a_x - b_x).abs() <= THRESHOLD {
            let mut dir_a = &a.start - &a.start;
            if dir_a.y() < 0. {
                dir_a *= -1.;
            }

            let mut dir_b = &b.start - &b.start;
            if dir_b.y() < 0. {
                dir_b *= -1.;
            }

            return dir_a.x().partial_cmp(&dir_b.x()).unwrap_or(Ordering::Equal);
        }

        a_x.partial_cmp(&b_x).unwrap_or(Ordering::Equal)
    }

    #[derive(Debug)]
    pub struct Event<'a> {
        pub point: Vector2f,

        /// If this event is triggered at the upper endpoint of a line segment,
        /// this is the corresponding line segment.
        pub segment: Option<&'a LineSegment2f>,
    }

    // Ascending y coordinate. If same y, order by ascending x.
    // TODO: Given that only store there are no issues with using threshold
    // comparison here while only storing one segment per event (if a == b and b ==
    // c, then that doesn't imply that a == c).
    impl<'a> Ord for Event<'a> {
        fn cmp(&self, other: &Self) -> Ordering {
            compare_points(&self.point, &other.point)
        }
    }

    impl<'a> PartialOrd for Event<'a> {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }

    impl<'a> PartialEq for Event<'a> {
        fn eq(&self, other: &Self) -> bool {
            self.cmp(other).is_eq()
        }
    }

    impl<'a> Eq for Event<'a> {}
}

/// A point intersection between two or more line segments.
#[derive(Debug, PartialEq)]
pub struct Intersection2f<'a> {
    pub point: Vector2f,
    pub segments: Vec<&'a LineSegment2f>,
}

fn vec2f(x: f32, y: f32) -> Vector2f {
    Vector2f::from_slice(&[x, y])
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn intersections_test() {
        let segments = vec![
            LineSegment2f {
                start: vec2f(0., 0.),
                end: vec2f(10., 10.),
            },
            LineSegment2f {
                start: vec2f(10., 0.),
                end: vec2f(0., 10.),
            },
            LineSegment2f {
                start: vec2f(0., 7.),
                end: vec2f(10., 7.),
            },
        ];

        let ints = LineSegment2f::intersections(&segments);

        println!("{:#?}", ints);

        assert_eq!(
            &ints,
            &[Intersection2f {
                point: vec2f(5., 5.),
                segments: vec![&segments[0], &segments[1]]
            }]
        );
    }
}
