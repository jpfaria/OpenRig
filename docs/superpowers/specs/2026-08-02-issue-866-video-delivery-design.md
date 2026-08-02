# Video delivery (#866, spec 3 of 5)

**Status:** design approved (owner, 2026-08-02); spec under review. The
build-vs-buy decision is deliberately left open — this spec exists to make it
with numbers.

**Series:** spec 3 of #866. Spec 2 defined `VideoRef::Hls { url }`; this spec
defines where that URL comes from and what it costs.

## Problem

A course's videos have to live somewhere, be transcoded into something a player
can stream, reach students at acceptable quality worldwide, and not be freely
downloadable by anyone who bought one lesson.

None of that is OpenRig code. It is a service the app talks to, and it is the
only piece of the whole platform with a **recurring cost per student per hour
watched**. Getting it wrong does not break the app; it breaks the business.

## Goals

1. A teacher uploads a video and it becomes streamable without them thinking
   about codecs.
2. A student's player receives a URL that works for them, now, and stops working
   afterwards.
3. The cost per student is known before the first course is sold.
4. The choice of provider is reversible — the app never learns who hosts the
   video.

## Non-goals

- DRM. The owner chose **time-limited signed URLs**: a link tied to the account
  that expires in minutes. It stops casual sharing; it does not stop screen
  recording, and nothing short of certified DRM does. Widevine/FairPlay would
  also force a certified player and kill the native Slint playback that makes
  the lesson screen worth having.
- Live streaming. Courses are recorded.
- The catalogue, accounts and purchase (spec 4). This spec assumes an
  authenticated student and an entitlement check it can call.

## The contract with the app

The app never talks to the video host. It asks the Hub for playback and gets
back a URL that already works:

```
GET /v1/lessons/{lesson_id}/playback
Authorization: Bearer <student token>

200 OK
{
  "url": "https://video.openrig.app/abc123/manifest.m3u8?token=…&expires=…",
  "expires_at": "2026-08-02T18:42:00Z",
  "duration_s": 812,
  "poster": "https://…/poster.jpg"
}

403 — the student has no entitlement to this course
404 — the lesson has no video yet
```

That is the whole interface. Everything below the line can be replaced without
touching a line of OpenRig, which is the point: **the app is not coupled to the
hosting decision.**

Expiry is short (minutes) and the app re-requests on resume, so a paused lesson
picked up the next day fetches a fresh URL rather than failing mid-playback. The
player must treat a 403 on segment fetch as "ask for a new URL", not as an error
to show the student.

## Two paths

### Path A — managed service under your own brand

Cloudflare Stream, Bunny Stream or Mux do ingest, transcode, storage, CDN and
signed URLs. The Hub stores an asset id; the student never sees the provider.

**What you operate:** nothing but the Hub's thin proxy that signs the URL.

### Path B — build it

Object storage (S3/R2) + a transcode worker (ffmpeg) + a CDN + your own URL
signing.

**What you operate:** a transcode queue, storage lifecycle, CDN configuration,
signing keys, and the monitoring for all of it.

## Cost

**These are modelled estimates from list prices looked up on 2026-08-02, not
measured numbers.** Re-confirm before committing — video pricing moves, and Mux
cut its rates significantly in 2026.

Assumptions, stated so they can be argued with:

- 720p at ~2.5 Mbps ≈ **1.1 GB per hour** of watching.
- An active student watches **30 min/day ≈ 900 min ≈ 16.5 GB per month**.
- A catalogue of **50 hours** of video, stored in three renditions.

**Per active student, per month:**

| Provider | Basis | Estimated cost |
|---|---|---|
| Bunny Stream | ~$0.01/GB delivered | **~$0.17** |
| Cloudflare Stream | $1 per 1,000 min delivered | **~$0.90** |
| Mux | ~$0.009/min delivered | **~$8.10** |
| DIY on CloudFront | ~$0.085/GB egress | **~$1.40** + transcode + storage + your time |

**Catalogue storage, per month:**

| Provider | Basis | Estimated cost |
|---|---|---|
| Bunny | $0.005/GB, ~165 GB in 3 renditions | **~$0.83** |
| Cloudflare Stream | $5 per 1,000 min stored, 3,000 min | **~$15** |

Two conclusions worth stating plainly:

1. **Delivery, not storage, is the cost.** Storage is rounding error at this
   catalogue size; what scales is minutes watched.
2. **The spread between providers is 50×.** At 100 active students, Bunny is
   ~$17/month and Mux is ~$810/month for the same viewing. Whatever else the
   decision weighs, it should not be made without this number in front of it.

Path B only beats Path A's cheapest option at volume, and only if operations
time is counted as free. At the scale where the first courses live, it is
strictly worse.

**Recommendation: Path A, on the cheapest provider that meets the quality bar,
behind the Hub's own URL** — so switching provider later is a Hub deployment,
not a course migration or an app release.

## Pipeline

```
teacher uploads ──► Hub ──► provider ingest ──► transcode (provider)
                     │                                │
                     │                          asset id + status
                     ▼                                ▼
              lesson record ◄───────────────── webhook: ready
                     │
   student opens ────┤
                     ▼
          entitlement check ──► sign URL ──► app plays HLS
```

The lesson holds a **status** (`uploading`, `processing`, `ready`, `failed`), so
the teacher tool (spec 5) can show progress and the client can say "this lesson
is still processing" rather than failing to play.

## Renditions

Three is enough for a guitar course: **1080p, 720p, 480p**. The content is a
person and a fretboard — detail on the fretting hand matters, motion does not.
Audio at 128 kbps AAC; the student is not judging tone through a lesson video,
they are judging it through their own rig.

Adaptive bitrate is the provider's job in Path A. In Path B it is a manifest you
generate and get to debug.

## What this changes in OpenRig

Almost nothing, by design:

- The HLS player from spec 2 already exists behind `LessonVideo`.
- Add: request playback before playing, handle expiry by re-requesting, and
  render the `processing`/`failed` states.

No new audio path, no new stream, no RT-thread involvement — video audio goes to
the system output as spec 2 established.

## Testing

- Playback request: entitled student gets a URL; unentitled gets 403; missing
  video gets 404.
- Expiry: a URL past `expires_at` triggers a re-request rather than an error
  shown to the student.
- Lesson status rendering: `processing` and `failed` states in the UI.
- The player against a fixture HLS manifest served locally — no provider needed
  in CI.
- Real-provider tests are manual and out of CI; they need credentials and cost
  money per run.

## Open

- Which provider, after re-confirming prices and testing quality from Brazil —
  latency and CDN presence there matter more than list price.
- Whether the Hub proxies playback (one hop, full control, its bandwidth) or
  redirects to a signed provider URL (no hop, provider sees the student's IP).
  Redirect is cheaper and is the default assumption above.
