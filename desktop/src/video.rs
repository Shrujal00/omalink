//! The phone's screen during mirroring: H.264 off the network, decoded here,
//! drawn by the Mirror page (see `videosink.cpp`).
//!
//! Frames are decoded on a thread of their own. When decoding falls behind,
//! frames are dropped up to the next one that can be decoded on its own, and
//! the phone is asked for one, so the picture stays current rather than late.

use std::sync::mpsc::{sync_channel, TrySendError};
use std::time::{Duration, Instant};

use omalink_net::iroh::endpoint::RecvStream;
use omalink_proto::net::VideoFrame;
use openh264::decoder::Decoder;
use openh264::formats::YUVSource;
use tokio::sync::mpsc::UnboundedSender;

use crate::core::Event;

extern "C" {
    fn omalink_video_frame(y: *const u8, y_stride: i32, u: *const u8, u_stride: i32, v: *const u8, v_stride: i32, width: i32, height: i32);
    fn omalink_video_clear();
}

/// How many encoded frames may wait for the decoder before some are dropped.
const BACKLOG: usize = 6;

pub fn clear() {
    unsafe { omalink_video_clear() };
}

/// Reads one mirroring stream until it ends.
pub async fn receive(mut recv: RecvStream, width: u32, height: u32, serial: u64, tx: UnboundedSender<Event>) {
    let (frames, queue) = sync_channel::<Vec<u8>>(BACKLOG);
    let events = tx.clone();
    let decoder = std::thread::Builder::new().name("omalink-decode".into()).spawn(move || {
        let Ok(mut decoder) = Decoder::new() else {
            let _ = events.send(Event::MirrorFailed { serial, reason: "Couldn't start the video decoder.".into() });
            return;
        };
        let (mut shown, mut since) = (0u32, Instant::now());
        let mut asked = Instant::now() - Duration::from_secs(5);
        while let Ok(packet) = queue.recv() {
            match decoder.decode(&packet) {
                Ok(Some(picture)) => {
                    let (w, h) = picture.dimensions();
                    let (ys, us, vs) = picture.strides();
                    unsafe {
                        omalink_video_frame(
                            picture.y().as_ptr(), ys as i32, picture.u().as_ptr(), us as i32, picture.v().as_ptr(), vs as i32, w as i32, h as i32,
                        )
                    };
                    shown += 1;
                }
                Ok(None) => {}
                // Lost its place; a frame that stands alone puts it back.
                Err(_) => {
                    if asked.elapsed() > Duration::from_secs(1) {
                        asked = Instant::now();
                        let _ = events.send(Event::MirrorKeyframe { serial });
                    }
                }
            }
            if since.elapsed() >= Duration::from_secs(1) {
                let _ = events.send(Event::MirrorFps { serial, fps: shown });
                shown = 0;
                since = Instant::now();
            }
        }
    });
    if decoder.is_err() {
        return;
    }
    let _ = tx.send(Event::MirrorLive { serial, width, height });

    let mut header = [0u8; VideoFrame::HEADER];
    // After a drop, frames that depend on the dropped one are useless.
    let mut skipping = false;
    loop {
        if recv.read_exact(&mut header).await.is_err() {
            break;
        }
        let (len, _pts, flags) = VideoFrame::parse(&header);
        if len > VideoFrame::MAX {
            break;
        }
        let mut packet = vec![0u8; len];
        if recv.read_exact(&mut packet).await.is_err() {
            break;
        }
        let standalone = flags & (VideoFrame::KEY | VideoFrame::CONFIG) != 0;
        if skipping && !standalone {
            continue;
        }
        skipping = false;
        match frames.try_send(packet) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                skipping = true;
                let _ = tx.send(Event::MirrorKeyframe { serial });
            }
            Err(TrySendError::Disconnected(_)) => break,
        }
    }
    drop(frames);
    let _ = tx.send(Event::MirrorStreamEnd { serial });
}
