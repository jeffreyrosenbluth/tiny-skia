// Copyright 2011 Google Inc.
// Copyright 2020 Evgeniy Reizner
//
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use crate::{Point, Rect};

use crate::scalar::Scalar;

pub const MAX_POINTS: usize = 4;

/// Clip the line pts[0]...pts[1] against clip, ignoring segments that
/// lie completely above or below the clip. For portions to the left or
/// right, turn those into vertical line segments that are aligned to the
/// edge of the clip.
///
/// Return the number of line segments that result, and store the end-points
/// of those segments sequentially in lines as follows:
///
/// 1st segment: lines[0]..lines[1]
/// 2nd segment: lines[1]..lines[2]
/// 3rd segment: lines[2]..lines[3]
pub fn clip<'a>(src: &[Point; 2], clip: &Rect, can_cull_to_the_right: bool, points: &'a mut [Point; MAX_POINTS]) -> &'a [Point] {
    let (mut index0, mut index1) = if src[0].y < src[1].y { (0, 1) } else { (1, 0) };

    // Check if we're completely clipped out in Y (above or below)

    if src[index1].y <= clip.top() {
        // we're above the clip
        return &[];
    }

    if src[index0].y >= clip.bottom() {
        // we're below the clip
        return &[];
    }

    // Chop in Y to produce a single segment, stored in tmp[0..1]

    let mut tmp = *src;

    // now compute intersections
    if src[index0].y < clip.top() {
        tmp[index0] = Point::from_xy(sect_with_horizontal(src, clip.top()), clip.top());
        debug_assert!(is_between_unsorted(tmp[index0].x, src[0].x, src[1].x));
    }

    if tmp[index1].y > clip.bottom() {
        tmp[index1] = Point::from_xy(sect_with_horizontal(src, clip.bottom()), clip.bottom());
        debug_assert!(is_between_unsorted(tmp[index1].x, src[0].x, src[1].x));
    }

    // Chop it into 1..3 segments that are wholly within the clip in X.

    // temp storage for up to 3 segments
    let mut result_storage = [Point::zero(); MAX_POINTS];
    let mut line_count = 1;
    let mut reverse;

    if src[0].x < src[1].x {
        index0 = 0;
        index1 = 1;
        reverse = false;
    } else {
        index0 = 1;
        index1 = 0;
        reverse = true;
    }

    let result: &[Point] = if tmp[index1].x <= clip.left() {
        // wholly to the left
        tmp[0].x = clip.left();
        tmp[1].x = clip.left();
        reverse = false;
        &tmp
    } else if tmp[index0].x >= clip.right() {
        // wholly to the right
        if can_cull_to_the_right {
            return &[];
        }

        tmp[0].x = clip.right();
        tmp[1].x = clip.right();
        reverse = false;
        &tmp
    } else {
        let mut offset = 0;

        if tmp[index0].x < clip.left() {
            result_storage[offset] = Point::from_xy(clip.left(), tmp[index0].y);
            offset += 1;
            result_storage[offset] = Point::from_xy(clip.left(), sect_clamp_with_vertical(tmp, clip.left()));
            debug_assert!(is_between_unsorted(result_storage[offset].y, tmp[0].y, tmp[1].y));
        } else {
            result_storage[offset] = tmp[index0];
        }
        offset += 1;

        if tmp[index1].x > clip.right() {
            result_storage[offset] = Point::from_xy(clip.right(), sect_clamp_with_vertical(tmp, clip.right()));
            debug_assert!(is_between_unsorted(result_storage[offset].y, tmp[0].y, tmp[1].y));
            offset += 1;
            result_storage[offset] = Point::from_xy(clip.right(), tmp[index1].y);
        } else {
            result_storage[offset] = tmp[index1];
        }

        line_count = offset;
        &result_storage
    };

    // Now copy the results into the caller's lines[] parameter
    if reverse {
        // copy the pts in reverse order to maintain winding order
        for i in 0..=line_count {
            points[line_count - i] = result[i];
        }
    } else {
        let len = line_count + 1;
        points[0..len].copy_from_slice(&result[0..len]);
    }

    &points[0..line_count+1]
}

/// Returns X coordinate of intersection with horizontal line at Y.
fn sect_with_horizontal(src: &[Point; 2], y: f32) -> f32 {
    let dy = src[1].y - src[0].y;
    if dy.is_nearly_zero() {
        src[0].x.ave(src[1].x)
    } else {
        // need the extra precision so we don't compute a value that exceeds
        // our original limits
        let x0 = f64::from(src[0].x);
        let y0 = f64::from(src[0].y);
        let x1 = f64::from(src[1].x);
        let y1 = f64::from(src[1].y);
        let result = x0 + (f64::from(y) - y0) * (x1 - x0) / (y1 - y0);

        // The computed X value might still exceed [X0..X1] due to quantum flux
        // when the doubles were added and subtracted, so we have to pin the
        // answer :(
        pin_unsorted_f64(result, x0, x1) as f32
    }
}

/// Returns value between the two limits, where the limits are either ascending or descending.
#[inline]
fn is_between_unsorted(value: f32, limit0: f32, limit1: f32) -> bool {
    if limit0 < limit1 {
        limit0 <= value && value <= limit1
    } else {
        limit1 <= value && value <= limit0
    }
}

#[inline]
fn sect_clamp_with_vertical(src: [Point; 2], x: f32) -> f32 {
    let y = sect_with_vertical(src, x);
    // Our caller expects y to be between src[0].y and src[1].y (unsorted), but due to the
    // numerics of floats/doubles, we might have computed a value slightly outside of that,
    // so we have to manually clamp afterwards.
    // See skbug.com/7491
    pin_unsorted_f32(y, src[0].y, src[1].y)
}

/// Returns Y coordinate of intersection with vertical line at X.
#[inline]
fn sect_with_vertical(src: [Point; 2], x: f32) -> f32 {
    let dx = src[1].x - src[0].x;
    if dx.is_nearly_zero() {
        src[0].y.ave(src[1].y)
    } else {
        // need the extra precision so we don't compute a value that exceeds
        // our original limits
        let x0 = f64::from(src[0].x);
        let y0 = f64::from(src[0].y);
        let x1 = f64::from(src[1].x);
        let y1 = f64::from(src[1].y);
        let result = y0 + (f64::from(x) - x0) * (y1 - y0) / (x1 - x0);
        result as f32
    }
}

#[inline]
fn pin_unsorted_f32(value: f32, mut limit0: f32, mut limit1: f32) -> f32 {
    if limit1 < limit0 {
        std::mem::swap(&mut limit0, &mut limit1);
    }
    // now the limits are sorted
    debug_assert!(limit0 <= limit1);

    if value < limit0 {
        limit0
    } else if value > limit1 {
        limit1
    } else {
        value
    }
}

#[inline]
fn pin_unsorted_f64(value: f64, mut limit0: f64, mut limit1: f64) -> f64 {
    if limit1 < limit0 {
        std::mem::swap(&mut limit0, &mut limit1);
    }
    // now the limits are sorted
    debug_assert!(limit0 <= limit1);

    if value < limit0 {
        limit0
    } else if value > limit1 {
        limit1
    } else {
        value
    }
}