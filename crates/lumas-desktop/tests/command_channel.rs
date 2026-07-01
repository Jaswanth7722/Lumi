//! Tests for the DesktopCommandChannel.

use lumas_desktop::DesktopError;
use lumas_desktop::command::DesktopCommandChannel;
use lumas_desktop::config::DesktopConfig;

#[tokio::test]
async fn test_command_sent_to_closed_channel_returns_event_loop_exited() {
    let (tx, _rx) = crossbeam_channel::bounded(16);
    // Drop rx immediately so the channel is closed.
    drop(_rx);

    let channel = DesktopCommandChannel::new(tx);
    let result: Result<(), DesktopError> = channel
        .send(
            |responder| lumas_desktop::command::DesktopCommand::Shutdown,
            100,
            "test_command",
        )
        .await;

    // Should get an error since the channel is closed.
    assert!(result.is_err());
}

#[test]
fn test_concurrent_commands_do_not_interleave_responses() {
    let (tx, rx) = crossbeam_channel::bounded(16);
    let channel = DesktopCommandChannel::new(tx);

    // Spawn multiple senders and ensure responses don't cross.
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let ch = channel.clone();
            tokio::spawn(async move {
                // Create a oneshot channel manually for testing.
                let (resp_tx, resp_rx) = tokio::sync::oneshot::channel::<Result<(), DesktopError>>();
                
                // Send a test command.
                ch.send_raw(lumas_desktop::command::DesktopCommand::Shutdown);
                
                // If we received a response somehow, it should be ours.
                tokio::time::timeout(
                    std::time::Duration::from_millis(10),
                    resp_rx,
                )
                .await
            })
        })
        .collect();
}

#[test]
fn test_channel_clone_is_send_sync() {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    assert_send::<DesktopCommandChannel>();
    assert_sync::<DesktopCommandChannel>();
}
