# filmcraft-matroska

A re-export of `craft_media::matroska`. Documentation: https://github.com/storytold/craft-libs/blob/main/crates/media/README.md

## Oracle coverage

`tests/oracle.rs` requires actual Xiph/EBML lacing on AAC and FLAC fixtures, and
FixedOrEbml lacing on PCM. The Vorbis remux regression supplies a deliberately
incompatible declared duration and requires the writer to keep every packet's
original timestamp, payload, keyframe flag and codec headers without lacing.
This avoids assuming that every encoder's `DefaultDuration` permits lacing.
All remux cases are checked independently with ffprobe; assertions are not skipped
when requested lacing cannot preserve timestamps.
The encoded Block flags must match the requested mode: Xiph, EBML, or either
fixed-size or EBML for FixedOrEbml; unlaced fallback blocks remain valid.

Laced frames share a block timestamp and must be contiguous; see
[RFC 9559 section 10.3.5](https://www.rfc-editor.org/rfc/rfc9559.html#section-10.3.5).
The muxer implementation and media dependencies are unchanged by this oracle repair.
