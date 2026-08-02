# Hub: courses (#866, spec 4 of 5)

**Status:** design approved (owner, 2026-08-02); spec under review.

**Series:** spec 4 of #866. Extends the OpenRig Hub of **#309** — a course is
another item type in the same registry, not a second marketplace. Decisions
taken here that also apply to plugins should be folded back into #309.

## Problem

A teacher records a course; a student finds it, buys it, and takes it inside
OpenRig. That needs a catalogue, identities on both sides, a payment that splits
between the teacher and the platform, and an entitlement the app can check
before it hands over a video URL.

This is the piece that turns OpenRig from an application into a platform, with
everything that implies — including obligations that do not exist today.

## Decisions taken (owner, 2026-08-02)

- **Revenue share.** The teacher sets the price, the Hub charges the student and
  retains a percentage.
- **Both sign-in methods:** email + password *and* OAuth (Google/GitHub). The
  student picks; the account is the same account.
- **Time-limited signed playback URLs** (spec 3), gated by entitlement.

## Inherited from #309, still open

Moderation policy, package signing, version compatibility ranges, offline
behaviour, and trademark responsibility. Courses raise the same questions as
plugins and should not be answered separately.

One that courses raise more sharply than plugins: **a plugin is data the student
keeps; a course is access that can be revoked.** If a teacher removes a course,
what happens to a student who paid for it? Answer this before the first sale,
not after the first complaint.

## Model

```
Account ── owns ──► Entitlement ──► Course (version-pinned)
   │
   └── may be ──► TeacherProfile ──► publishes ──► Course ──► Lesson[]
                                                      │
                                                      └─► video asset (spec 3)
```

**Entitlement is per course, not per lesson**, and pins the course version the
student bought. A teacher publishing a new version does not silently change what
someone paid for; the student is offered the update.

## Surfaces

### Catalogue

Browse and search, filtered by instrument, level and language. A course page
shows the teacher, the syllabus, the price, sample lessons, and — the thing that
distinguishes this from every other course platform — **what the course actually
measures**: the rubric it holds you to (spec 1). "This course expects 90% clean at
120 BPM" is information a student cannot get anywhere else, and it is the
honest version of "beginner/intermediate/advanced".

### Student account

Sign-in by email+password or OAuth, purchase history, entitlements, and progress
sync. Progress lives locally (spec 2, ADR 0003 system scope); syncing it to the
account is what lets a student move between their laptop and the Orange Pi rig
and keep their place. **Sync is opt-in** — grades are personal data.

### Teacher account

Profile, courses, versions, sales, payouts. Publishing goes through the teacher
tool (spec 5), which uploads the package and the videos.

## Payments and revenue share

The Hub is the merchant: the student pays the Hub, the Hub pays the teacher
minus its share.

This makes you a **financial intermediary**, and that is a real obligation, not
a checkbox:

- Payment processing, refunds and chargebacks.
- Tax on the sale, which varies by the student's country.
- Payouts to teachers, which means collecting their tax identity — and for
  Brazilian teachers, the reporting that goes with it.
- Fraud and disputes.

**Recommendation: use a merchant-of-record provider** (Paddle, Lemon Squeezy,
Stripe with the tax and Connect products) rather than being the merchant
yourself. They take a larger cut and remove nearly all of the above. At the
volume of a first course, the difference in fee is far smaller than the cost of
getting tax handling wrong.

The revenue-share percentage is a business decision, not an architectural one;
the system stores it per course so it can differ per teacher without a
deployment.

## API the app talks to

```
POST /v1/auth/login | /v1/auth/oauth/{provider}
GET  /v1/catalogue?instrument=guitar&level=beginner
GET  /v1/courses/{id}
POST /v1/courses/{id}/purchase
GET  /v1/me/entitlements
GET  /v1/courses/{id}/package        → the .orcourse archive (entitled only)
GET  /v1/lessons/{id}/playback       → signed video URL (spec 3)
POST /v1/me/progress                 → opt-in sync
```

The app is a thin client over this: it installs a package, plays lessons, and
evaluates locally. **Everything that matters to the student works offline once
the course is installed** — except video, which is remote by definition. The
evaluator, the score, the metronome and the rig all run with no network.

## Publishing

1. The teacher tool (spec 5) uploads the package and the videos.
2. Videos transcode (spec 3); lessons show `processing` until ready.
3. The course goes to review, if moderation says so (open, per #309).
4. Published, versioned semver, immutable at that version.

## What this changes in OpenRig

- A Hub client: catalogue browsing, sign-in, purchase hand-off to the browser,
  install, update.
- Sign-in stored in system config; **never in the `.openrig`**, which travels.
- Everything else already exists from spec 2.

Purchase itself opens the browser rather than collecting a card in a Slint
window — the app should never handle payment details.

## Testing

- Entitlement: entitled student gets the package; unentitled gets 403; expired
  or refunded entitlement loses access.
- Version pinning: a new course version does not change an existing
  entitlement's content until the student accepts.
- Offline: an installed course opens, plays exercises and records progress with
  the network down; only video fails, and it fails with a message rather than a
  broken screen.
- The client against a fake Hub in CI; no live service, no credentials.

## Risks

| Risk | Mitigation |
|---|---|
| Becoming a financial intermediary | Merchant-of-record provider; the Hub never touches card data |
| A teacher unpublishes a course students paid for | Decide the policy before the first sale; the recommended answer is that entitlements survive unpublishing at the pinned version |
| Personal data (grades, progress) crossing borders | Sync is opt-in and off by default; progress is local first |
| Course quality with open publishing | Inherits #309's moderation decision; the rubric is public on the course page, so a course that measures nothing is visibly a course that measures nothing |
| Platform obligations distracting from the app | This spec is the boundary: the app stays a thin client, and everything here can be built after the app already works with local courses |
