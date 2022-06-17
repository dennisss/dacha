use alloc::vec::Vec;
use common::tree::attribute::EmptyAttribute;
use core::cmp::Ordering;

use common::tree::avl::AVLTree;
use common::tree::binary_heap::BinaryHeap;
use common::tree::comparator::Comparator;
use common::InRange;

use crate::geometry::line::Line2;
use crate::geometry::quantized::PseudoAngle;
use crate::matrix::cwise_binary_ops::{CwiseMax, CwiseMin};
use crate::matrix::element::{ElementType, FloatElementType};
use crate::matrix::{vec2f, Matrix2f, MatrixStatic, Vector2};

/// Bounded 2-dimensional line segment defined by two endpoints which are
/// connected. The two endpoints are inclusive (considered to be part of the
/// segment).
#[derive(Debug, PartialEq, Clone)]
pub struct LineSegment2<T: FloatElementType> {
    pub start: Vector2<T>,
    pub end: Vector2<T>,
}

impl<T: FloatElementType> LineSegment2<T> {
    pub fn contains(&self, point: &Vector2<T>) -> bool {
        let line = Line2::from_points(&self.start, &self.end);

        if line.distance_to_point(point) > intersections::THRESHOLD.into() {
            return false;
        }

        // Verify in the segment bbox.
        let min = (&self.start).cwise_min(&self.end) - (intersections::THRESHOLD / 2.);
        let max = (&self.start).cwise_max(&self.end) + (intersections::THRESHOLD / 2.);
        point >= &min && point <= &max
    }

    /// Computes the intersection point of the current line segment with
    /// another.
    ///
    /// Unlike a general line intersection, the intersection point must be
    /// inside of each segment to be returned.
    pub fn intersect(&self, other: &Self) -> Option<Vector2<T>> {
        let current_line = Line2::from_points(&self.start, &self.end);
        let other_line = Line2::from_points(&other.start, &other.end);

        let mut point = match current_line.intersect(&other_line) {
            Some(p) => p,
            None => {
                return None;
            }
        };

        // If the intersection point is close to an endpoint, clip it to that endpoint.
        // This way an intersection computed on connected line segments returns the
        // exactly correct point.
        for p in [&self.start, &self.end, &other.start, &other.end] {
            if compare_points(&point, &p).is_eq() {
                point = p.clone();
                break;
            }
        }

        // Checks that the point is in the bounding box of the segment.
        // We already know that the point is on the line of the segment.
        let on_segment = |segment: &LineSegment2<T>, point: &Vector2<T>| -> bool {
            let min = (&segment.start).cwise_min(&segment.end) - T::from(intersections::THRESHOLD);
            let max = (&segment.start).cwise_max(&segment.end) + T::from(intersections::THRESHOLD);
            point >= &min && point <= &max
        };

        if !on_segment(self, &point) || !on_segment(other, &point) {
            return None;
        }

        Some(point)
    }

    /// Finds all intersections between a set of line segments.
    ///
    /// Internally uses the Bentley-Ottmann algorithm.
    ///
    /// TODO: what should this return if there are overlapping segments?
    /// ^ Should emit the any endpoints of either line that are also present on
    /// the other line.
    ///
    /// TODO: For each intersection, we also want to know which segments where
    /// involved (one or more segment indices)
    ///
    /// Returns all intersection points between the segments in order of
    /// increasing y then increasing x.
    pub fn intersections(segments: &[Self]) -> Vec<Intersection2<T>> {
        use self::intersections::*;

        let mut output = vec![];

        // Ordered set of points which we want to visit next. We sweep a line from low
        // to high y values.
        //
        // TODO: Switch to an AVL tree and de-duplicate insertions ahead of time
        // (otherwise this may grow excessively large due to lines becoming adjacent and
        // then not-adjacent and then adjacent again due to interleaved lines).
        let mut event_queue = BinaryHeap::<Event<T>>::default();
        for (i, segment) in segments.iter().enumerate() {
            let (upper, lower) = upper_lower_endpoints(segment);

            event_queue.insert(Event {
                point: upper,
                segment: Some(i),
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
        let mut sweep_status =
            AVLTree::<LineSegmentIndex, EmptyAttribute, LineSweepComparator<T>>::new(
                LineSweepComparator {
                    segments,
                    event_point: Vector2::zero(),
                },
            );

        while let Some(first_event) = event_queue.extract_min() {
            let event_point = first_event.point;

            // List of all segments whose upper endpoint is at this event point (this are
            // all not yet in the sweep_status and just in consecutive equal event points).
            let mut upper_segments = vec![];
            {
                if let Some(segment) = first_event.segment {
                    upper_segments.push(segment);
                }
                while let Some(next_event) = event_queue.peek_min() {
                    // TODO: Consider comparing to latest event point with the larger y value that
                    // also matches as there is a change that we extract a lower line segment
                    // endpoint before an upper line segment endpoint.
                    // (although when using quantized points, this is less likely)
                    //
                    // NOTE: This must use a threshold as we want to ensure that we consider lines
                    // that start at the intersection point.
                    if compare_points(&event_point, &next_event.point).is_eq() {
                        if let Some(segment) = next_event.segment.clone() {
                            upper_segments.push(segment);
                        }

                        event_queue.extract_min();
                    } else {
                        break;
                    }
                }
            }

            let new_comparator = LineSweepComparator {
                segments,
                event_point: event_point.clone(),
            };

            let mut existing_segments = vec![];
            {
                let mut iter = sweep_status.lower_bound_by(&event_point, &new_comparator);

                while let Some(segment) = iter.next().cloned() {
                    if new_comparator.compare(&segment, &event_point).is_ne() {
                        break;
                    }

                    existing_segments.push(segment);
                }
            }

            // Remove all segments that we touched (will be re-inserted in the
            // next step).
            // NOTE: We use the last sweep point in the comparator to ensure search
            // stability.
            for segment in existing_segments.iter().cloned() {
                let v = &segments[sweep_status.remove(&segment).unwrap()];
                assert_eq!(v.start, segments[segment].start);
                assert_eq!(v.end, segments[segment].end);
            }

            // We should have removed all discrepancies between the new and old sweep lines
            // in the above loop so we can now completely switch to comparing using the new
            // one.
            sweep_status.change_comparator(new_comparator.clone());

            /// XXX: At this point, we can change the comparator.
            // Of the segments we are about to insert, this tracks the left most and right
            // most ones.
            let mut first_last_segment = None;

            // (Re-)Insert all segments which had an upper endpoint as the
            // current segment or was already in the sweep status and has an intersection in
            for segment_idx in upper_segments
                .iter()
                .cloned()
                .chain(existing_segments.iter().cloned())
            {
                let segment = &segments[segment_idx];

                // Don't insert any segments with the lower endpoint equal to the current event
                // point (this is how segments eventually get removed from the status).
                let (_, lower) = upper_lower_endpoints(segment);
                if compare_points(&event_point, &lower).is_eq() {
                    continue;
                }

                sweep_status.insert(segment_idx);

                first_last_segment = Some(match first_last_segment.take() {
                    Some((mut first, mut last)) => {
                        if new_comparator.compare(&segment_idx, &first).is_lt() {
                            first = segment_idx;
                        }
                        if new_comparator.compare(&segment_idx, &last).is_gt() {
                            last = segment_idx;
                        }

                        (first, last)
                    }
                    None => (segment_idx, segment_idx),
                });
            }

            // TODO: If the above insertions and removals cause any line segments to stop
            // being adjacent to each other, remove their intersection points from the event
            // queue.

            let mut intersection_left_neighbor = None;
            let mut intersection_right_neighbor = None;

            if let Some((first, last)) = first_last_segment {
                // NOTE: unwrap() should never fail if all the logic is correct as we just
                // inserted these
                let mut first_iter = sweep_status.find(&first).unwrap();
                let mut last_iter = sweep_status.find(&last).unwrap();

                // TODO: Verify that compare_segments_at_sweep_line is
                // sufficienctly robust that segments that aren't exactly equal
                // don't get compared as Ordering::Equal. Otherwise we will need
                // to continue advancing the iterators forward/reverse to skip
                // over any other equal segments.

                assert_eq!(first_iter.prev(), Some(&first)); // Skip the 'first'
                intersection_left_neighbor = first_iter.peek().cloned();

                assert_eq!(last_iter.next(), Some(&last)); // Skip over 'last'
                intersection_right_neighbor = last_iter.peek().cloned();

                if let Some(first_neighbor) = intersection_left_neighbor.clone() {
                    if let Some(next_point) = find_intersection_event(
                        &segments[first],
                        &segments[first_neighbor],
                        &event_point,
                    ) {
                        event_queue.insert(Event {
                            point: next_point,
                            segment: None,
                        });
                    }
                }

                if let Some(last_neighbor) = intersection_right_neighbor.clone() {
                    if let Some(next_point) = find_intersection_event(
                        &segments[last],
                        &segments[last_neighbor],
                        &event_point,
                    ) {
                        event_queue.insert(Event {
                            point: next_point,
                            segment: None,
                        });
                    }
                }
            } else {
                let mut iter = sweep_status.lower_bound(&event_point);

                // TODO: If we hit the end of the tree, this needs to be sufficiently robust to
                // be able to seek backwards from there.
                intersection_right_neighbor = iter.prev().cloned();
                intersection_left_neighbor = iter.peek().cloned();

                if intersection_right_neighbor.is_some() && intersection_left_neighbor.is_some() {
                    if let Some(next_point) = find_intersection_event(
                        &segments[intersection_right_neighbor.unwrap()],
                        &segments[intersection_left_neighbor.unwrap()],
                        &event_point,
                    ) {
                        event_queue.insert(Event {
                            point: next_point,
                            segment: None,
                        });
                    }
                }
            }

            // Report an intersection
            if upper_segments.len() + existing_segments.len() > 1 {
                let mut segments = vec![];
                segments.extend_from_slice(&upper_segments);
                segments.extend_from_slice(&existing_segments);

                output.push(Intersection2 {
                    point: event_point.clone(),
                    segments,
                    left_neighbor: intersection_left_neighbor,
                    right_neighbor: intersection_right_neighbor,
                });
            }
        }

        output
    }

    /// Slower version of Self::intersections() of time complexity O(n^2) for
    /// 'n' segments. This implementation is simpler though and less likely to
    /// be buggy.
    pub fn intersections_slow(segments: &[Self]) -> Vec<Vector2<T>> {
        // TODO: Use an AVL tree to store intersections and later dedup them.
        let mut output = vec![];

        for i in 0..segments.len() {
            for j in (i + 1)..segments.len() {
                if let Some(point) = segments[i].intersect(&segments[j]) {
                    output.push(point);
                }
            }
        }

        output
    }
}

mod intersections {

    use crate::geometry::line::Line2;

    use super::*;

    pub const THRESHOLD: f32 = 1e-3;

    pub type LineSegmentIndex = usize;

    pub fn upper_lower_endpoints<T: FloatElementType>(
        segment: &LineSegment2<T>,
    ) -> (Vector2<T>, Vector2<T>) {
        let mut upper_point = segment.start.clone();
        let mut lower_point = segment.end.clone();
        if compare_points(&upper_point, &lower_point).is_gt() {
            core::mem::swap(&mut upper_point, &mut lower_point);
        }

        (upper_point, lower_point)
    }

    #[derive(Debug, Clone)]
    pub struct LineSweepComparator<'a, T: FloatElementType> {
        pub segments: &'a [LineSegment2<T>],
        pub event_point: Vector2<T>,
    }

    impl<'a, T: FloatElementType>
        common::tree::comparator::Comparator<LineSegmentIndex, LineSegmentIndex>
        for LineSweepComparator<'a, T>
    {
        fn compare(&self, a: &LineSegmentIndex, b: &LineSegmentIndex) -> Ordering {
            let ord = compare_segments_at_sweep_line(
                &self.segments[*a],
                &self.segments[*b],
                &self.event_point,
            );

            // To ensure that we can retrieve any segment after it is inserted, only a
            // segment i should be equal to itself and non others.
            if ord.is_eq() {
                return a.cmp(b);
            }

            ord
        }
    }

    // This form of the comparator is used for finding all intersections at event
    // points so needs to compare with a threshold as intersections with each line
    // segment are in-exact.
    impl<'a, T: FloatElementType> common::tree::comparator::Comparator<LineSegmentIndex, Vector2<T>>
        for LineSweepComparator<'a, T>
    {
        fn compare(&self, segment: &LineSegmentIndex, point: &Vector2<T>) -> Ordering {
            let x = sweep_line_x(&self.segments[*segment], &self.event_point);
            if (point.x() - x).abs() <= THRESHOLD.into() {
                return Ordering::Equal;
            }

            x.partial_cmp(&point.x()).unwrap()
        }
    }

    /// Computes the 'x' coordinate of the given 'segment' when we intersect a
    /// horizontal line at 'point.y()'.
    ///
    /// In the case that 'segment' is horizontal, we return the closest point on
    /// the segment to 'point.x()'.
    pub fn sweep_line_x<T: FloatElementType>(segment: &LineSegment2<T>, point: &Vector2<T>) -> T {
        let x = {
            if (segment.end.y() - segment.start.y()).abs() < THRESHOLD.into() {
                point.x()
            } else {
                let t = (point.y() - segment.start.y()) / (segment.end.y() - segment.start.y());
                t * (segment.end.x() - segment.start.x()) + segment.start.x()
            }
        };

        let min_x = segment.start.x().min(segment.end.x());
        let max_x = segment.start.x().max(segment.end.x());
        x.min(max_x).max(min_x)
    }

    pub fn find_intersection_event<T: FloatElementType>(
        a: &LineSegment2<T>,
        b: &LineSegment2<T>,
        event_point: &Vector2<T>,
    ) -> Option<Vector2<T>> {
        let intersection = match a.intersect(b) {
            Some(p) => p,
            None => return None,
        };

        // Ignore intersections occuring before the current event point.
        if compare_points(&intersection, &event_point).is_le() {
            return None;
        }

        Some(intersection)
    }

    // TODO: Ideally this would only return Equal if the line segments are exactly
    // equal
    //
    // TODO: Verify passing 2 horizontal lines that are equal always
    // returns an equal return.
    //
    // TODO: If two distinct horizontal lines are passed, ensure that we have a
    // commutative behavior.
    pub fn compare_segments_at_sweep_line<T: FloatElementType>(
        a: &LineSegment2<T>,
        b: &LineSegment2<T>,
        point: &Vector2<T>,
    ) -> Ordering {
        if a.start == b.start && a.end == b.end {
            return Ordering::Equal;
        }

        let a_x = sweep_line_x(a, point);
        let b_x = sweep_line_x(b, point);

        fn normalize_direction<T: FloatElementType>(v: &mut Vector2<T>) {
            if v.y().abs() < intersections::THRESHOLD.into() {
                // Normalizing direction of a horizontal line.
                // Avoid small negative y offsets.
                v[1] = T::zero();
                if v.x() > T::zero() {
                    *v *= T::from(-1.);
                }
            } else {
                if v.y() < T::zero() {
                    *v *= T::from(-1.);
                }
            }
        }

        // When both segments are intersecting at the sweep line, we must sort the
        // segments based on their values immediately below the sweep line.
        //
        // To do this we compare the x value of their direction vectors to tell which
        // will move left or right after crossing the intersection (heading towards
        // decreasing y values).
        if (a_x - b_x).abs() <= THRESHOLD.into() {
            // TODO: If both lines are horizontal, compare based on their min x

            let mut dir_a = &a.start - &a.end;
            let mut dir_b = &b.start - &b.end;

            // Make the angles with the +x axis between 0 and pi.
            // Horizontal lines should be pointed towards greater event points.
            normalize_direction(&mut dir_a);
            normalize_direction(&mut dir_b);

            let angle_a = dir_a.pseudo_angle();
            let angle_b = dir_b.pseudo_angle();

            // TODO: Check this.
            let mut ordering = angle_a.partial_cmp(&angle_b).unwrap();

            // If the event point hasn't yet reached the intersection point, then we
            // actually want to use the ordering above the intersection point.
            let event_before_intersection =
                compare_points(&point, &Vector2::from_slice(&[a_x, point.y()])).is_lt();
            if event_before_intersection {
                ordering = ordering.reverse();
            }

            return ordering;
        }

        a_x.partial_cmp(&b_x).unwrap()
    }

    #[derive(Debug)]
    pub struct Event<T: FloatElementType> {
        pub point: Vector2<T>,

        /// If this event is triggered at the upper endpoint of a line segment,
        /// this is the index of the corresponding line segment.
        pub segment: Option<LineSegmentIndex>,
    }

    // Descending y coordinate. If same y, order by ascending x.
    // TODO: Given that only store there are no issues with using threshold
    // comparison here while only storing one segment per event (if a == b and b ==
    // c, then that doesn't imply that a == c).
    impl<T: FloatElementType> Ord for Event<T> {
        fn cmp(&self, other: &Self) -> Ordering {
            compare_points(&self.point, &other.point)
        }
    }

    impl<T: FloatElementType> PartialOrd for Event<T> {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }

    impl<T: FloatElementType> PartialEq for Event<T> {
        fn eq(&self, other: &Self) -> bool {
            self.cmp(other).is_eq()
        }
    }

    impl<T: FloatElementType> Eq for Event<T> {}
}

/// Line sweep ordering relationship for two points.
///
/// The 'smallest' points have the highest y values. At the same y value, the
/// smaller x value is first.
pub fn compare_points<T: FloatElementType + PartialOrd>(
    a: &Vector2<T>,
    b: &Vector2<T>,
) -> Ordering {
    if (a.y() - b.y()).abs() <= intersections::THRESHOLD.into() {
        if (a.x() - b.x()).abs() <= intersections::THRESHOLD.into() {
            Ordering::Equal
        } else {
            a.x().partial_cmp(&b.x()).unwrap_or(Ordering::Equal)
        }
    } else {
        b.y().partial_cmp(&a.y()).unwrap_or(Ordering::Equal)
    }
}

pub fn compare_points_i64(a: &Vector2<i64>, b: &Vector2<i64>) -> Ordering {
    if a.y() == b.y() {
        return a.x().cmp(&b.x());
    }

    b.y().cmp(&a.y())
}

/// The smallest point will be the left-most point. If multiple points share the
/// same x, then the one with lowest y will be selected.
pub fn compare_points_x_then_y(a: &Vector2<i64>, b: &Vector2<i64>) -> Ordering {
    if a.x() == b.x() {
        return a.y().partial_cmp(&b.y()).unwrap();
    }

    a.x().partial_cmp(&b.x()).unwrap()

    /*
    if (a.x() - b.x()).abs() <= intersections::THRESHOLD {
        if (a.y() - b.y()).abs() <= intersections::THRESHOLD {
            Ordering::Equal
        } else {
            a.y().partial_cmp(&b.y()).unwrap()
        }
    } else {
        a.x().partial_cmp(&b.x()).unwrap()
    }
    */
}

/// A point intersection between two or more line segments.
#[derive(Debug, PartialEq, Clone)]
pub struct Intersection2<T: FloatElementType> {
    pub point: Vector2<T>,

    /// Index of each segment which contains the intersection point. Will
    /// contain at least 2 elements. These will not be in any particular order.
    pub segments: Vec<usize>,

    /// Index of the line segment immediately to the left of this intersection.
    ///
    /// If the LOWER endpoint of a line segment is at the y position of this
    /// intersection, it will not be counted when searching for this neighbor.
    pub left_neighbor: Option<usize>,

    /// Index of the line segment immediately to the right of this intersection.
    ///
    /// If the UPPER endpoint of a line segment is at the y position of this
    /// intersection, it will not be counted when searching for this neighbor.
    pub right_neighbor: Option<usize>,
}

#[cfg(test)]
mod tests {

    use super::*;

    // TODO: Test a single horizontal line intersecting with 4 vertical lines (1 at
    // each endpoint and 2 in the middle)

    #[test]
    fn sweep_line_x_test() {
        use super::intersections::sweep_line_x;

        let a = LineSegment2 {
            start: vec2f(0., 0.),
            end: vec2f(10., 10.),
        };

        assert_eq!(sweep_line_x(&a, &vec2f(0., 0.)), 0.);
        assert_eq!(sweep_line_x(&a, &vec2f(0., 1.)), 1.);
        assert_eq!(sweep_line_x(&a, &vec2f(0., 5.)), 5.);

        let a = LineSegment2 {
            start: vec2f(294., 199.),
            end: vec2f(493., 343.),
        };
        assert_eq!(sweep_line_x(&a, &vec2f(294., 199.)), 294.);
        assert_eq!(sweep_line_x(&a, &vec2f(493., 343.)), 493.);
    }

    #[test]
    fn comparing_perpendicular_lines() {
        let a = LineSegment2 {
            start: vec2f(0., 20.),
            end: vec2f(0., 0.),
        };
        let b = LineSegment2 {
            start: vec2f(20., 20.),
            end: vec2f(0., 20.),
        };

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&a, &b, &vec2f(0., 20.)),
            Ordering::Less
        );

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&b, &a, &vec2f(0., 20.)),
            Ordering::Greater
        );

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&a, &b, &vec2f(20., 20.)),
            Ordering::Less
        );

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&b, &a, &vec2f(20., 20.)),
            Ordering::Greater
        );

        // TODO: Flip 'start' and 'end' and verify things behave the same.

        let bp = LineSegment2 {
            start: b.end,
            end: b.start,
        };

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&a, &bp, &vec2f(0., 20.)),
            Ordering::Less
        );

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&bp, &a, &vec2f(0., 20.)),
            Ordering::Greater
        );

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&a, &bp, &vec2f(20., 20.)),
            Ordering::Less
        );

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&bp, &a, &vec2f(20., 20.)),
            Ordering::Greater
        );
    }

    #[test]
    fn sort_below_sweep_line() {
        // ------- Sweep line starts here.
        //
        // \     /
        //  \   /
        //   \ /
        //    /
        //   / \
        //  /   \
        // /a    \b

        let a = LineSegment2 {
            start: vec2f(0., 0.),
            end: vec2f(10., 10.),
        };
        let b = LineSegment2 {
            start: vec2f(10., 0.),
            end: vec2f(0., 10.),
        };

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&a, &b, &vec2f(11., 11.)),
            Ordering::Greater
        );

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&a, &b, &vec2f(-1., -1.)),
            Ordering::Less
        );

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&a, &b, &vec2f(4.9, 4.9)),
            Ordering::Less
        );

        // As seen as get near the sweep line, the ordering flips because the lines have
        // intersected are now going in different directions.
        assert_eq!(
            intersections::compare_segments_at_sweep_line(&a, &b, &vec2f(5., 5.)),
            Ordering::Less
        );

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&a, &b, &vec2f(5.1, 5.1)),
            Ordering::Greater
        );
    }

    #[test]
    fn compare_lines_diverging_in_same_direction() {
        let a = LineSegment2 {
            start: vec2f(0.0, 20.0),
            end: vec2f(5.0, 15.0),
        };

        let b = LineSegment2 {
            start: vec2f(0.0, 20.0),
            end: vec2f(5.0, 5.0),
        };

        let point = vec2f(0.0, 20.0);

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&a, &b, &point),
            Ordering::Greater
        );

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&b, &a, &point),
            Ordering::Less
        );
    }

    #[test]
    fn comparing_before_the_intersection_point() {
        let a = LineSegment2 {
            start: vec2f(276.0, 657.0),
            end: vec2f(209.0, 655.0),
        };
        let b = LineSegment2 {
            start: vec2f(209.0, 655.0),
            end: vec2f(145.0, 666.0),
        };

        let before_intersection = vec2f(100., 655.);

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&a, &b, &before_intersection),
            Ordering::Greater
        );

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&b, &a, &before_intersection),
            Ordering::Less
        );
    }

    #[test]
    fn horizontal_comparison() {
        let a = LineSegment2 {
            start: vec2f(10., 0.),
            end: vec2f(0., 10.),
        };

        let b = LineSegment2 {
            start: vec2f(0., 7.),
            end: vec2f(10., 7.),
        };

        let point = vec2f(10., 7.);

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&a, &b, &point),
            Ordering::Less
        );
    }

    #[test]
    fn horizontal_comparison2() {
        let a = LineSegment2 {
            start: vec2f(0., 0.),
            end: vec2f(10., 10.),
        };

        let b = LineSegment2 {
            start: vec2f(0., 7.),
            end: vec2f(10., 7.),
        };

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&a, &b, &vec2f(0., 7.)),
            Ordering::Greater
        );

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&a, &b, &vec2f(7., 7.)),
            Ordering::Less
        );

        assert_eq!(
            intersections::compare_segments_at_sweep_line(&a, &b, &vec2f(10., 7.)),
            Ordering::Less
        );
    }

    #[test]
    fn intersections_test() {
        let segments = vec![
            LineSegment2 {
                start: vec2f(0., 0.),
                end: vec2f(10., 10.),
            },
            LineSegment2 {
                start: vec2f(10., 0.),
                end: vec2f(0., 10.),
            },
            LineSegment2 {
                start: vec2f(0., 7.),
                end: vec2f(10., 7.),
            },
            LineSegment2 {
                start: vec2f(7., 6.),
                end: vec2f(7., 10.),
            },
        ];

        assert_eq!(
            &LineSegment2::intersections(&segments[0..2]),
            &[Intersection2 {
                point: vec2f(5., 5.),
                segments: vec![1, 0],
                left_neighbor: None,
                right_neighbor: None,
            },]
        );

        assert_eq!(
            &LineSegment2::intersections(&segments[0..3]),
            &[
                Intersection2 {
                    point: vec2f(3., 7.),
                    segments: vec![2, 1],
                    left_neighbor: None,
                    right_neighbor: Some(0),
                },
                Intersection2 {
                    point: vec2f(7., 7.),
                    segments: vec![2, 0],
                    left_neighbor: Some(1),
                    right_neighbor: None,
                },
                Intersection2 {
                    point: vec2f(5., 5.),
                    segments: vec![1, 0],
                    left_neighbor: None,
                    right_neighbor: None,
                },
            ]
        );

        assert_eq!(
            &LineSegment2::intersections(&segments),
            &[
                Intersection2 {
                    point: vec2f(3., 7.),
                    segments: vec![2, 1],
                    left_neighbor: None,
                    right_neighbor: Some(3),
                },
                Intersection2 {
                    point: vec2f(7., 7.),
                    segments: vec![2, 3, 0],
                    left_neighbor: Some(1),
                    right_neighbor: None,
                },
                Intersection2 {
                    point: vec2f(5., 5.),
                    segments: vec![1, 0],
                    left_neighbor: None,
                    right_neighbor: None,
                },
            ]
        );
    }

    #[test]
    fn inexact_intersection() {
        let segments = vec![
            LineSegment2 {
                start: vec2f(294., 199.),
                end: vec2f(493., 343.),
            },
            LineSegment2 {
                start: vec2f(481., 183.),
                end: vec2f(300., 354.),
            },
        ];

        assert_eq!(
            &LineSegment2::intersections(&segments),
            &[Intersection2 {
                point: vec2f(390.3027, 268.6864),
                segments: vec![1, 0],
                left_neighbor: None,
                right_neighbor: None,
            }]
        );
    }

    #[test]
    fn quad_intersections() {
        let segments = vec![
            LineSegment2 {
                // Right-ish
                start: vec2f(209.0, 247.0),
                end: vec2f(433.0, 441.0),
            },
            LineSegment2 {
                // Left-most
                start: vec2f(427.0, 229.0),
                end: vec2f(186.0, 461.0),
            },
            LineSegment2 {
                // Left-ish
                start: vec2f(434.0, 340.0),
                end: vec2f(321.0, 457.0),
            },
            LineSegment2 {
                // Right-most
                start: vec2f(335.0, 266.0),
                end: vec2f(449.0, 420.0),
            },
        ];

        // let expected = LineSegment2::intersections_slow(&segments);
        let ints = LineSegment2::intersections(&segments);

        assert_eq!(
            &ints,
            &[
                Intersection2 {
                    point: vec2f(380.42773, 395.4687,),
                    segments: vec![2, 0],
                    left_neighbor: Some(1),
                    right_neighbor: Some(3),
                },
                Intersection2 {
                    point: vec2f(408.9665, 365.91965,),
                    segments: vec![2, 3,],
                    left_neighbor: Some(0),
                    right_neighbor: None,
                },
                Intersection2 {
                    point: vec2f(313.9139, 337.8629,),
                    segments: vec![1, 0],
                    left_neighbor: None,
                    right_neighbor: Some(3),
                },
                Intersection2 {
                    point: vec2f(357.28812, 296.10852,),
                    segments: vec![1, 3,],
                    left_neighbor: Some(0),
                    right_neighbor: None,
                },
            ]
        );
    }

    #[test]
    fn intersect_at_lower_endpoint() {
        // This stresses the left/right neighbor code as the intersection point
        // min/max segment no longer exist in the sweep status tree.

        //       0      1
        //        \    /
        //    2 \  \  /  / 3
        //       \  \/  /
        //        \    /
        //         \  /
        //          \/

        let segments = vec![
            LineSegment2 {
                start: vec2f(0., 2.),
                end: vec2f(-2., 5.),
            },
            LineSegment2 {
                start: vec2f(0., 2.),
                end: vec2f(2., 5.),
            },
            LineSegment2 {
                start: vec2f(0., 0.),
                end: vec2f(-2., 3.),
            },
            LineSegment2 {
                start: vec2f(0., 0.),
                end: vec2f(2., 3.),
            },
        ];

        assert_eq!(
            &LineSegment2::intersections(&segments),
            &[
                Intersection2 {
                    point: vec2f(0., 2.),
                    segments: vec![0, 1],
                    left_neighbor: Some(2),
                    right_neighbor: Some(3),
                },
                Intersection2 {
                    point: vec2f(0., 0.),
                    segments: vec![2, 3],
                    left_neighbor: None,
                    right_neighbor: None,
                },
            ]
        );

        assert_eq!(
            &LineSegment2::intersections(&segments[0..2]),
            &[Intersection2 {
                point: vec2f(0., 2.),
                segments: vec![0, 1],
                left_neighbor: None,
                right_neighbor: None,
            },]
        );

        assert_eq!(
            &LineSegment2::intersections(&segments[0..3]),
            &[Intersection2 {
                point: vec2f(0., 2.),
                segments: vec![0, 1],
                left_neighbor: Some(2),
                right_neighbor: None,
            },]
        );

        assert_eq!(
            &LineSegment2::intersections(&vec![
                segments[0].clone(),
                segments[1].clone(),
                segments[3].clone()
            ]),
            &[Intersection2 {
                point: vec2f(0., 2.),
                segments: vec![0, 1],
                left_neighbor: None,
                right_neighbor: Some(2),
            },]
        );
    }

    #[test]
    fn overlapping_horizontal_lines() {
        let segments = vec![
            LineSegment2 {
                start: vec2f(10., 0.),
                end: vec2f(20., 0.),
            },
            LineSegment2 {
                start: vec2f(15., 0.),
                end: vec2f(25., 0.),
            },
        ];

        assert_eq!(
            &LineSegment2::intersections(&segments),
            &[
                Intersection2 {
                    point: vec2f(15., 0.),
                    segments: vec![1, 0],
                    left_neighbor: None,
                    right_neighbor: None,
                },
                Intersection2 {
                    point: vec2f(20., 0.),
                    segments: vec![0, 1],
                    left_neighbor: None,
                    right_neighbor: None,
                },
            ]
        );

        let segments = vec![
            LineSegment2 {
                start: vec2f(10., 0.),
                end: vec2f(20., 0.),
            },
            LineSegment2 {
                start: vec2f(10., 0.),
                end: vec2f(25., 0.),
            },
        ];

        assert_eq!(
            &LineSegment2::intersections(&segments),
            &[
                Intersection2 {
                    point: vec2f(10., 0.),
                    segments: vec![0, 1],
                    left_neighbor: None,
                    right_neighbor: None,
                },
                Intersection2 {
                    point: vec2f(20., 0.),
                    segments: vec![0, 1],
                    left_neighbor: None,
                    right_neighbor: None,
                },
            ]
        );

        let segments = vec![
            LineSegment2 {
                start: vec2f(10., 0.),
                end: vec2f(20., 0.),
            },
            LineSegment2 {
                start: vec2f(0., 0.),
                end: vec2f(20., 0.),
            },
        ];

        assert_eq!(
            &LineSegment2::intersections(&segments),
            &[
                Intersection2 {
                    point: vec2f(10., 0.),
                    segments: vec![0, 1],
                    left_neighbor: None,
                    right_neighbor: None,
                },
                Intersection2 {
                    point: vec2f(20., 0.),
                    segments: vec![0, 1],
                    left_neighbor: None,
                    right_neighbor: None,
                },
            ]
        );
    }

    #[test]
    fn overlapping_colinear_lines() {
        let segments = vec![
            LineSegment2 {
                start: vec2f(0., 0.),
                end: vec2f(5., 5.),
            },
            LineSegment2 {
                start: vec2f(3., 3.),
                end: vec2f(8., 8.),
            },
        ];

        assert_eq!(
            &LineSegment2::intersections(&segments),
            &[
                Intersection2 {
                    point: vec2f(5., 5.),
                    segments: vec![0, 1],
                    left_neighbor: None,
                    right_neighbor: None,
                },
                Intersection2 {
                    point: vec2f(3., 3.),
                    segments: vec![0, 1],
                    left_neighbor: None,
                    right_neighbor: None,
                },
            ]
        );
    }

    // TODO: Also test that colinear lines that don't overlap don't trigger
    // intersections
}
