use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use herdr_client::protocol::PaneId;
use serde_json::{Value, json};

use super::{PAINT_FRAME, ScreenWatch};

fn write_frame(stream: &mut UnixStream, channel: u32, kind: u8, payload: &[u8]) {
    let mut frame = channel.to_le_bytes().to_vec();
    frame.push(kind);
    frame.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_le_bytes());
    frame.extend_from_slice(payload);
    stream.write_all(&frame).unwrap();
}

/// A stand-in for herdr-core that answers one watch with `frames`, each sent the moment the one
/// before was, then the frames `later` hands it, ending the stream once `later` ends, and tells
/// `credited` of each credit it gets.
fn core_sending(
    frames: Vec<Vec<u8>>,
    later: mpsc::Receiver<Vec<u8>>,
    credited: mpsc::Sender<u32>,
) -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("core.sock");
    let listener = UnixListener::bind(&path).unwrap();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let hello: Value = serde_json::from_str(&line).unwrap();
        let answer = json!({"jsonrpc": "2.0", "id": hello["id"], "result": {"framed": true}});
        writeln!(stream, "{answer}").unwrap();
        let mut header = [0; 9];
        reader.read_exact(&mut header).unwrap();
        let mut attach = vec![0; u32::from_le_bytes(header[5..9].try_into().unwrap()) as usize];
        reader.read_exact(&mut attach).unwrap();
        let attach: Value = serde_json::from_slice(&attach).unwrap();
        let attached = json!({"jsonrpc": "2.0", "id": attach["id"], "result": {"channel": 1}});
        write_frame(&mut stream, 0, 0, attached.to_string().as_bytes());
        for frame in frames {
            write_frame(&mut stream, 1, 0, &frame);
        }
        let mut writer = stream.try_clone().unwrap();
        thread::spawn(move || {
            for frame in later {
                write_frame(&mut writer, 1, 0, &frame);
            }
            // The stream ends once the test has nothing more to send.
            let _ = writer.shutdown(std::net::Shutdown::Write);
        });
        while reader.read_exact(&mut header).is_ok() {
            let mut payload = [0; 4];
            reader.read_exact(&mut payload).unwrap();
            let _ = credited.send(u32::from_le_bytes(payload));
        }
    });
    (directory, path)
}

#[test]
fn a_print_right_after_the_paint_is_told_as_a_print_and_not_painted() {
    let (_later, later) = mpsc::channel();
    let (credits, _credited) = mpsc::channel();
    let (_directory, socket) = core_sending(
        vec![b"painted".to_vec(), b"printed".to_vec()],
        later,
        credits,
    );

    let (paint, mut watch) = ScreenWatch::open(&socket, &PaneId("t1".into())).unwrap();

    assert_eq!(paint, "painted");
    watch.printed().unwrap();
}

#[test]
fn a_paint_of_full_frames_is_read_whole_and_each_frame_credited() {
    let full = vec![b'a'; PAINT_FRAME];
    let (_later, later) = mpsc::channel();
    let (credits, credited) = mpsc::channel();
    let (_directory, socket) = core_sending(vec![full.clone(), b"end".to_vec()], later, credits);

    let (paint, _watch) = ScreenWatch::open(&socket, &PaneId("t1".into())).unwrap();

    assert_eq!(paint.len(), PAINT_FRAME + 3);
    assert!(paint.ends_with("end"));
    let credits: Vec<u32> = (0..2)
        .map(|_| credited.recv_timeout(Duration::from_secs(5)).unwrap())
        .collect();
    assert_eq!(credits, [u32::try_from(PAINT_FRAME).unwrap(), 3]);
}

#[test]
fn a_watch_tells_a_print_only_once_the_program_printed() {
    let (later_frames, later) = mpsc::channel();
    let (credits, _credited) = mpsc::channel();
    let (_directory, socket) = core_sending(vec![b"painted".to_vec()], later, credits);
    let (_, mut watch) = ScreenWatch::open(&socket, &PaneId("t1".into())).unwrap();

    later_frames.send(b"printed".to_vec()).unwrap();
    watch.printed().unwrap();
    // The stand-in core ends the stream once it sent all it was given: a paint counted as a print
    // would leave the print above for this wait.
    drop(later_frames);
    assert!(
        watch.printed().is_err(),
        "the watch told a print the program never made"
    );
}
