use crate::{Session, commands::rate_stretch_preview};
use filmcraft_project::ClipId;
use filmcraft_time::Tick;
use serde_json::json;

fn fixture() -> (Session, Vec<ClipId>) {
    let mut s = Session::default();
    s.execute("file.newProject", json!({"name":"Ripple stretch"})).unwrap();
    s.execute("file.newSequence", json!({"name":"Sequence", "fps":25, "video":2, "audio":2})).unwrap();
    let r = s.execute("file.newOfflineFile", json!({"name":"Synthetic", "seconds":10, "fps":25, "video":true, "audio":true})).unwrap();
    let item = r["item"].as_u64().unwrap();
    let f = s.sequence_rate().tick_of(1).0;
    let mut ids = Vec::new();
    for n in 0..3 {
        let r = s.execute("timeline.place", json!({"item":item, "track":"V1", "audioTrack":"A1", "time":f*60*n, "sourceIn":f*20, "duration":f*60})).unwrap();
        ids.extend(r["clips"].as_array().unwrap().iter().map(|c| ClipId(c.as_u64().unwrap())));
    }
    s.execute("timeline.place", json!({"item":item, "track":"A2", "time":0, "duration":f*240})).unwrap();
    s.execute("timeline.setTrack", json!({"track":"A2", "locked":true})).unwrap();
    (s, ids)
}

#[test]
fn ripple_rate_stretch_keeps_linked_media_locked_music_and_one_undo_step() {
    for delta in [-20, 20] {
        for selected in [false, true] {
            let (mut s, ids) = fixture();
            if selected {
                s.execute("timeline.select", json!({"clips":[ids[2].0, ids[4].0]})).unwrap();
            }
            let before = s.project.clone();
            let history = s.history.undo.len();
            let d = s.sequence_rate().tick_of(delta);
            let preview = rate_stretch_preview(&s, ids[0], filmcraft_edit::Edge::Out, d, true).unwrap();
            assert_eq!(*s.project, *before, "preview is read only");
            assert_eq!(s.history.undo.len(), history);
            let r = s.execute("timeline.rateStretch", json!({"clip":ids[0].0, "edge":"out", "deltaFrames":delta, "ripple":true})).unwrap();
            let q = s.active_sequence().unwrap();
            assert_eq!(q, &preview, "release matches preview exactly");
            let old = before.sequence(s.state.active_sequence.unwrap()).unwrap();
            assert_eq!(q.audio_tracks[1], old.audio_tracks[1]);
            let f = s.sequence_rate().tick_of(1);
            for (n, c) in ids.iter().enumerate() {
                let now = q.find_item(*c).unwrap().1;
                let was = old.find_item(*c).unwrap().1;
                assert_eq!((now.source_in, now.source_out()), (was.source_in, was.source_out()));
                if n < 2 {
                    assert_eq!(now.duration, Tick(f.0 * (60 + delta)));
                    assert!((now.speed - 60.0 / (60 + delta) as f64).abs() < 1e-12);
                } else {
                    assert_eq!(now.start, was.start + d);
                    assert_eq!(now.source_time_at(now.start), was.source_time_at(was.start));
                }
            }
            assert!((r["speed"].as_f64().unwrap() - 60.0 / (60 + delta) as f64).abs() < 1e-12);
            let after = s.project.clone();
            let reopened = filmcraft_format::decode(&filmcraft_format::encode(&after, true)).unwrap().project;
            assert_eq!(reopened.sequence(s.state.active_sequence.unwrap()), Some(q), "saved state retains source ranges and rates");
            assert_eq!(s.history.undo.len(), history + 1);
            s.execute("edit.undo", json!({})).unwrap();
            assert_eq!(*s.project, *before);
            s.execute("edit.redo", json!({})).unwrap();
            assert_eq!(*s.project, *after);
        }
    }
}

#[test]
fn ordinary_stretch_and_dialog_ripple_do_not_enable_later_visual_ripple() {
    let (mut s, ids) = fixture();
    s.execute("clip.speedDuration", json!({"clips":[ids[0].0], "speed":200, "ripple":true})).unwrap();
    let before = s.project.clone();
    s.execute("timeline.rateStretch", json!({"clip":ids[0].0, "edge":"out", "deltaFrames":-10})).unwrap();
    let q = s.active_sequence().unwrap();
    for c in &ids[2..] {
        assert_eq!(q.find_item(*c), before.sequence(s.state.active_sequence.unwrap()).unwrap().find_item(*c));
    }
}

#[test]
fn unsupported_in_edge_and_hostile_commands_are_atomic() {
    let (mut s, ids) = fixture();
    for p in [
        json!({"clip":ids[0].0,"edge":"in","deltaFrames":5,"ripple":true}),
        json!({"clip":999999,"deltaFrames":5,"ripple":true}),
        json!({"clip":ids[0].0,"delta":i64::MAX,"ripple":true}),
    ] {
        let before = s.project.clone();
        let history = s.history.undo.len();
        assert!(s.execute("timeline.rateStretch", p).is_err());
        assert_eq!(*s.project, *before);
        assert_eq!(s.history.undo.len(), history);
    }
    s.execute("timeline.setTrack", json!({"track":"V1","locked":true})).unwrap();
    let before = s.project.clone();
    assert!(s.execute("timeline.rateStretch", json!({"clip":ids[0].0,"deltaFrames":5,"ripple":true})).is_err());
    assert_eq!(*s.project, *before);
}

/// A separate synthetic CPU-render check for the reported temporary black clip. This checks
/// source coverage, not the desktop's asynchronous decoder/cache path or the reporter's media.
#[test]
fn synthetic_speed_duration_ripple_still_renders_after_undo_redo() {
    let mut s = Session::default();
    s.execute("file.openDemoProject", json!({})).unwrap();
    let item = s.active_sequence().unwrap().video_tracks[0].items[0].item;
    s.execute("file.newSequence", json!({"name":"Render check","fps":25,"width":64,"height":64,"video":1,"audio":0})).unwrap();
    let r = s.execute("timeline.place", json!({"item":item.0,"track":"V1","audio":false,"time":0,"duration":s.sequence_rate().tick_of(60).0})).unwrap();
    let c = ClipId(r["clips"][0].as_u64().unwrap());
    let original = s.render_program_at(1.0, Tick::ZERO).unwrap();
    for speed in [50, 200] {
        s.execute("clip.speedDuration", json!({"clips":[c.0],"speed":speed,"ripple":true})).unwrap();
        for _ in 0..2 {
            let clip = s.active_sequence().unwrap().find_item(c).unwrap().1;
            let duration = clip.duration;
            for t in [Tick::ZERO, Tick(duration.0 / 2), duration - s.sequence_rate().frame_duration()] {
                let (image, offline) = s.try_render_program_with_offline_at(1.0, t).unwrap();
                assert!(offline.is_empty());
                assert!(image.px.as_chunks::<4>().0.iter().any(|p| p[0] > 1e-4 || p[1] > 1e-4 || p[2] > 1e-4));
            }
            s.execute("edit.undo", json!({})).unwrap();
            assert_eq!(s.render_program_at(1.0, Tick::ZERO).unwrap().px, original.px);
            s.execute("edit.redo", json!({})).unwrap();
        }
        s.execute("edit.undo", json!({})).unwrap();
    }
}

#[test]
fn caption_refusal_matches_preview_and_preserves_history() {
    let (mut s, ids) = fixture();
    s.execute("captions.add", json!({"text":"Synthetic caption","frame":80,"durationSeconds":1})).unwrap();
    let before = s.project.clone();
    let history = s.history.undo.len();
    let delta = s.sequence_rate().tick_of(-10);
    assert!(rate_stretch_preview(&s, ids[0], filmcraft_edit::Edge::Out, delta, true).is_err());
    assert!(s.execute("timeline.rateStretch", json!({"clip":ids[0].0,"edge":"out","delta":delta.0,"ripple":true})).is_err());
    assert_eq!(*s.project, *before);
    assert_eq!(s.history.undo.len(), history);
}
