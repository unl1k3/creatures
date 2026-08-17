# Physics Test Scenarios

These layouts are the reference cases for validating living blobs, corpses, and
irregular support surfaces. Test each one with a large and a small blob and
observe it for at least ten seconds after contact settles.

## A. Living Blob on a Corpse

```text
       ( living )
      (  corpse  )
=======================
```

- The upper blob adapts to the corpse instead of becoming a perfect circle.
- Both contours remain separated without visible overlap.
- The upper blob can charge and perform a jump.
- The corpse can deform and shift, but cannot breathe or accept input.
- Neither body develops persistent trembling after settling.

## B. Side Push Against a Corpse

```text
 ( living )  ->  ( corpse )       edge
====================================|
```

- Momentum is transferred to the corpse.
- The corpse rolls or slides instead of behaving like a fixed wall.
- Pushing it beyond the edge makes it fall under gravity.
- Neither contour penetrates the other during sustained pushing.

## C. Narrow Pedestal

```text
          ( blob )
             ||
             ||
=============||=============
```

- Contact is recognized only near the pedestal top.
- The blob falls when its centre moves outside the support region.
- Side contact does not enable jump charging.
- The membrane does not wrap through either top corner.

## D. Stair Transition

```text
                         ______
                  ______|
           ______|
__________|
```

- The blob climbs without snagging a membrane point on a corner.
- Ground contact remains stable between consecutive steps.
- A corpse can be pushed down the staircase and participates in impacts.
- Resting contact does not accumulate collision damage.

## E. Semicircular Support

```text
            ( blob )
          .-----------.
       .-'             '-.
____.-'___________________'-.___
```

- Lower membrane points form one continuous support patch.
- The blob partially conforms to the curve instead of remaining circular.
- It rolls down when its centre leaves the top region.
- The averaged contact normal changes smoothly along the arc.
- Charging is allowed near the top, but not on the steep sides.

This scenario must use a genuine curved or tessellated convex collider. A drawn
semicircle with a rectangular physical surface is not a valid test.

## Acceptance Metrics

Record maximum penetration depth, ground-contact count and span, centre drift,
peak-to-peak movement during the final two seconds, self-intersection count, and
whether jump charging agrees with the averaged support normal.

A settled stack passes when it has no self-intersections, no visible overlap,
and no sustained oscillation. Numerical tolerances should be fixed from the
first reproducible baseline capture rather than from visual appearance alone.
