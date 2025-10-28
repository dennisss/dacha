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
use crate::matrix::element::{ElementType, ErrorEpsilon, FloatElementType, ScalarElementType};
use crate::matrix::{vec2f, Matrix2f, MatrixStatic, Vector2};

/// Bounded 2-dimensional line segment defined by two endpoints which are
/// connected. The two endpoints are inclusive (considered to be part of the
/// segment).
#[derive(Debug, PartialEq, Clone)]
pub struct LineSegment2<T: ScalarElementType> {
    pub start: Vector2<T>,
    pub end: Vector2<T>,
}

impl<T: FloatElementType> LineSegment2<T> {
    pub fn contains(&self, point: &Vector2<T>, max_error: T) -> bool {
        let line = Line2::from_points(&self.start, &self.end);

        if line.distance_to_point(point) > max_error {
            return false;
        }

        // Verify in the segment bbox.
        let min = (&self.start).cwise_min(&self.end) - (max_error / T::from(2.));
        let max = (&self.start).cwise_max(&self.end) + (max_error / T::from(2.));
        point >= &min && point <= &max
    }
}

impl<T: ScalarElementType + ErrorEpsilon> LineSegment2<T> {
    /// Computes the intersection point of the current line segment with
    /// another.
    ///
    /// Unlike a general line intersection, the intersection point must be
    /// inside of each segment to be returned.
    pub fn intersect(&self, other: &Self, max_error: T) -> Option<Vector2<T>> {
        let current_line = Line2::from_points(&self.start, &self.end);
        let other_line = Line2::from_points(&other.start, &other.end);

        // TODO: Pass some error threshold into this.
        // TODO: Need a better algorithm (like intersect_segments_exact) for this since
        // it is likely to go out of bounds.
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
            let min = (&segment.start).cwise_min(&segment.end) - max_error;
            let max = (&segment.start).cwise_max(&segment.end) + max_error;
            point >= &min && point <= &max
        };

        if !on_segment(self, &point) || !on_segment(other, &point) {
            return None;
        }

        Some(point)
    }

    pub fn intersect_exact(&self, other: &Self) -> Option<Vector2<T>> {
        let current_line = Line2::from_points(&self.start, &self.end);
        let other_line = Line2::from_points(&other.start, &other.end);

        current_line.intersect_segments_exact(&other_line)
    }

    /// Computes the 'x' value for the given 'y' coordinate on this line
    /// segment. Will return None if the 'y' is not on the line segment.
    pub fn evaluate_at_y(&self, y: T) -> Option<T> {
        let line = Line2::from_points(&self.start, &self.end);

        let horiz = Line2 {
            base: Vector2::from_slice(&[T::zero(), y]),
            dir: Vector2::from_slice(&[T::one(), T::zero()]),
        };

        let t = match line.intersection_coeff_unchecked(&horiz) {
            Some(v) => v,
            None => return None,
        };

        if t[0] < T::zero() || t[0] > T::one() {
            return None;
        }

        let pt = line.evaluate(t[0]);
        assert_eq!(pt[1], y);

        Some(pt[0])
    }

    /// Finds all intersections between a set of line segments.
    ///
    /// If two line segments are overlapping, this will report two points (each
    /// will be an endpoint )
    ///
    /// Internally uses the Bentley-Ottmann algorithm.
    /// - In order for the algorithm to be stable, all internal comparisons and
    ///   intersection point calculations are performed exactly.
    /// - 'T' needs to be a type supporting exact arithmetic like 'Rational' for
    ///   this to work.
    /// - Note that with Rationals, arbitrary
    ///   multiplication/addition/subtraction has a high change of overflow, so
    ///   this function will internally try to avoid doing those operations.
    ///
    /// TODO: Verify that the type this is run on supports exact arithmetic.
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
        let mut event_queue = BinaryHeap::<Event<T>, EventComparator>::new(EventComparator {}, ());
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

            let existing_segments = {
                let mut existing_segments = vec![];

                let mut iter = sweep_status.lower_bound_by(&event_point, &new_comparator);

                while let Some(segment) = iter.next().cloned() {
                    if new_comparator.compare(&segment, &event_point).is_ne() {
                        break;
                    }

                    existing_segments.push(segment);
                }

                existing_segments
            };

            /*
            let existing_segments = {
                let mut existing_segments = vec![];

                for segment in sweep_status.iter() {
                    if new_comparator.compare(segment, &event_point).is_eq() {
                        existing_segments.push(segment.clone());   
                    }
                }

                existing_segments
            };
            */

            // Remove all segments that we touched (will be re-inserted in the
            // next step).
            // NOTE: We use the last sweep point in the comparator to ensure search
            // stability.
            for segment in existing_segments.iter().cloned() {
                // TODO: This line is still crashing with exact dtypes.
                let v = &segments[sweep_status.remove(&segment).unwrap()];
                assert_eq!(v.start, segments[segment].start);
                assert_eq!(v.end, segments[segment].end);
            }

            // Debugging only sanity check that changing the comparator is a no-op.
            /*
            {
                let mut iter = sweep_status.iter();
                let mut last_value = None;
                while let Some(v) = iter.next() {
                    if let Some(last_value) = last_value.take() {
                        if new_comparator.compare(last_value, v) != Ordering::Less {
                            println!("{} , {}", *last_value, *v);

                            println!(
                                "I: {:?}",
                                segments[*last_value].intersect(&segments[*v], max_error)
                            );

                            {
                                let s1 = &segments[*v];
                                let s2 = &segments[*last_value];

                                let current_line = Line2::from_points(&s1.start, &s1.end);
                                let other_line = Line2::from_points(&s2.start, &s2.end);

                                // TODO: Pass some error threshold into this.
                                let mut point = current_line.intersect(&other_line);

                                println!("IR: {:?}", point);
                            }
                            /*


                            */

                            panic!("{:?} !< {:?}", segments[*last_value], segments[*v]);
                        }
                    }

                    last_value = Some(v);
                }
            }
            */

            // We should have removed all discrepancies between the new and old sweep lines
            // in the above loop so we can now completely switch to comparing using the new
            // one.
            sweep_status.change_comparator(new_comparator.clone());

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
                let mut segments_indices = vec![];
                segments_indices.extend_from_slice(&upper_segments);
                segments_indices.extend_from_slice(&existing_segments);

                output.push(Intersection2 {
                    point: event_point.clone(),
                    segments: segments_indices,
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
    pub fn intersections_slow(segments: &[Self], max_error: T) -> Vec<Vector2<T>> {
        // TODO: Use an AVL tree to store intersections and later dedup them.
        let mut output = vec![];

        for i in 0..segments.len() {
            for j in (i + 1)..segments.len() {
                if let Some(point) = segments[i].intersect(&segments[j], max_error) {
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

    pub type LineSegmentIndex = usize;

    pub fn upper_lower_endpoints<T: ScalarElementType + ErrorEpsilon>(
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
    pub struct LineSweepComparator<'a, T: ScalarElementType + ErrorEpsilon> {
        pub segments: &'a [LineSegment2<T>],
        pub event_point: Vector2<T>,
    }

    impl<'a, T: ScalarElementType + ErrorEpsilon>
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
    impl<'a, T: ScalarElementType + ErrorEpsilon>
        common::tree::comparator::Comparator<LineSegmentIndex, Vector2<T>>
        for LineSweepComparator<'a, T>
    {
        fn compare(&self, segment: &LineSegmentIndex, point: &Vector2<T>) -> Ordering {
            // TODO: We are still getting haivng this overflow.
            // Ideally we compare without computing the intercept.
            let x = sweep_line_x(&self.segments[*segment], &self.event_point);

            x.partial_cmp(&point.x()).unwrap()
        }
    }

    /// Computes the 'x' coordinate of the given 'segment' when we intersect a
    /// horizontal line at 'point.y()'.
    ///
    /// In the case that 'segment' is horizontal, we return the closest point on
    /// the segment to 'point.x()'.
    pub fn sweep_line_x<T: ScalarElementType + ErrorEpsilon>(
        segment: &LineSegment2<T>,
        point: &Vector2<T>,
    ) -> T {
        let x = {
            if segment.end.y() == segment.start.y() {
                let min_x = segment.start.x().min(segment.end.x());
                let max_x = segment.start.x().max(segment.end.x());
                point.x().min(max_x).max(min_x)
            } else {
                let mut t = (point.y() - segment.start.y()) / (segment.end.y() - segment.start.y());

                // 't' can end up being very large for near intersecting lines, so clamp to
                // avoid overflowing arithmetic.
                //
                // TODO: Eventually use '.clamp_between_0_to_1()'
                t = t.min(T::one()).max(T::zero());

                // TODO: If I just use the point for comparing values, I don't think I actually
                // need to compute this entirely.

                // TODO: This overflows.
                t * (segment.end.x() - segment.start.x()) + segment.start.x()
            }
        };

        x
    }

    pub fn find_intersection_event<T: ScalarElementType + ErrorEpsilon>(
        a: &LineSegment2<T>,
        b: &LineSegment2<T>,
        event_point: &Vector2<T>,
    ) -> Option<Vector2<T>> {
        let intersection = match a.intersect_exact(b) {
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
    pub fn compare_segments_at_sweep_line<T: ScalarElementType + ErrorEpsilon>(
        a: &LineSegment2<T>,
        b: &LineSegment2<T>,
        point: &Vector2<T>,
    ) -> Ordering {
        if a.start == b.start && a.end == b.end {
            return Ordering::Equal;
        }

        // TODO: Ideally need to be able to do comparison without actually fully
        // evaluating these.
        let a_x = sweep_line_x(a, point);
        let b_x = sweep_line_x(b, point);

        // println!("X S: {:?} ; {:?}", a_x, b_x);

        let normalize_direction = |v: &mut Vector2<T>| {
            if v.y() == T::zero() {
                // Normalizing direction of a horizontal line.
                // Avoid small negative y offsets.
                v[1] = T::zero();
                if v.x() > T::zero() {
                    *v *= T::from(-1);
                }
            } else {
                if v.y() < T::zero() {
                    *v *= T::from(-1);
                }
            }
        };

        // When both segments are intersecting at the sweep line, we must sort the
        // segments based on their values immediately below the sweep line.
        //
        // To do this we compare the x value of their direction vectors to tell which
        // will move left or right after crossing the intersection (heading towards
        // decreasing y values).
        if a_x == b_x {
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
    pub struct Event<T: ScalarElementType> {
        pub point: Vector2<T>,

        /// If this event is triggered at the upper endpoint of a line segment,
        /// this is the index of the corresponding line segment.
        pub segment: Option<LineSegmentIndex>,
    }

    pub struct EventComparator {}

    // Descending y coordinate. If same y, order by ascending x.
    // TODO: Given that only store there are no issues with using threshold
    // comparison here while only storing one segment per event (if a == b and b ==
    // c, then that doesn't imply that a == c).
    impl<T: ScalarElementType> Comparator<Event<T>> for EventComparator {
        fn compare(&self, a: &Event<T>, b: &Event<T>) -> Ordering {
            compare_points(&a.point, &b.point)
        }
    }
}

/// Line sweep ordering relationship for two points.
///
/// The 'smallest' points have the highest y values. At the same y value, the
/// smaller x value is first.
pub fn compare_points<T: ScalarElementType + PartialOrd>(
    a: &Vector2<T>,
    b: &Vector2<T>,
) -> Ordering {
    if a.y() == b.y() {
        if a.x() == b.x() {
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
pub struct Intersection2<T: ScalarElementType> {
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

    use crate::{matrix::vec2, rational::Rational};

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
            &LineSegment2::intersections(
                &vec![
                    segments[0].clone(),
                    segments[1].clone(),
                    segments[3].clone()
                ],
            ),
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

    #[test]
    fn interestions_example1() {
        let segment_data: &'static [(f32, f32, f32, f32)] = &[
            (0.0, 49.05, 0.942, 39.481),
            (0.942, 39.481, 3.734, 30.28),
            (3.734, 30.28, 8.266, 21.799),
            (8.266, 21.799, 14.367, 14.366),
            (14.367, 14.366, 21.799, 8.266),
            (21.799, 8.266, 30.28, 3.734),
            (30.28, 3.734, 39.481, 0.942),
            (39.481, 0.942, 49.05, 0.0),
            (49.05, 0.0, 58.62, 0.942),
            (58.62, 0.942, 67.821, 3.734),
            (67.821, 3.734, 76.301, 8.266),
            (76.301, 8.266, 83.734, 14.367),
            (83.734, 14.367, 89.834, 21.799),
            (89.834, 21.799, 94.367, 30.28),
            (94.367, 30.28, 97.158, 39.481),
            (97.158, 39.481, 98.101, 49.05),
            (98.101, 49.05, 97.158, 58.62),
            (97.158, 58.62, 94.367, 67.821),
            (94.367, 67.821, 89.834, 76.301),
            (89.834, 76.301, 83.734, 83.734),
            (83.734, 83.734, 76.301, 89.834),
            (76.301, 89.834, 67.821, 94.367),
            (67.821, 94.367, 58.62, 97.158),
            (58.62, 97.158, 49.05, 98.101),
            (49.05, 98.101, 39.481, 97.158),
            (39.481, 97.158, 30.28, 94.367),
            (30.28, 94.367, 21.799, 89.834),
            (21.799, 89.834, 14.366, 83.734),
            (14.366, 83.734, 8.266, 76.301),
            (8.266, 76.301, 3.734, 67.821),
            (3.734, 67.821, 0.942, 58.62),
            (0.942, 58.62, 0.0, 49.05),
        ];

        let mut segments = vec![];
        for (a, b, c, d) in segment_data.iter().cloned() {
            segments.push(LineSegment2 {
                start: Vector2::from_slice(&[a, b]),
                end: Vector2::from_slice(&[c, d]),
            })
        }

        let inter1 = LineSegment2::intersections_slow(&segments, 0.0001);

        // TODO: Fix this test
        /*
        let inter2 = LineSegment2::intersections(&segments, 0.0001);

        let mut n = 0;
        for i in &inter2 {
            n += i.segments.len() - 1;
        }

        assert_eq!(inter1.len(), n);
        */
    }

    #[test]
    fn overflow_intersection() {
        let segments = vec![
            LineSegment2 {
                start: vec2(Rational::from(0), Rational::from(0)),
                end: vec2(Rational::from(1000000), Rational::from(1000)),
            },
            LineSegment2 {
                start: vec2(Rational::from(100), Rational::from(0)),
                end: vec2(Rational::from(1000100), Rational::from(999)),
            },
        ];

        let inters = LineSegment2::intersections(&segments);
        assert_eq!(inters.len(), 0);
    }

    // TODO: Also test that colinear lines that don't overlap don't trigger
    // intersections
}
