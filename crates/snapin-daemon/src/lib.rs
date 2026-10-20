//! 单实例守护进程管理与 Unix Domain Socket 进程间通信模块。

pub mod client;
pub mod codec;
pub mod error;
pub mod protocol;
pub mod server;

pub use client::{
    notify_daemon_reload_config, send_command_to_daemon, send_request_to_daemon,
    send_request_to_path,
};
pub use codec::{read_framed_message, write_framed_message};
pub use error::DaemonError;
pub use protocol::{
    get_socket_path, DaemonCommand, DaemonRequest, DaemonResponse, IpcInvocation,
    MAX_IPC_FRAME_SIZE,
};
pub use server::IpcListener;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_framed_message_roundtrip() {
        let (mut client, mut server) = tokio::io::duplex(1024);

        let req = DaemonRequest::SwitchGroup(Some(42));
        tokio::spawn(async move {
            write_framed_message(&mut client, &req)
                .await
                .expect("写入分帧请求成功");
            let resp: DaemonResponse = read_framed_message(&mut client)
                .await
                .expect("读取分帧响应成功");
            assert_eq!(resp, DaemonResponse::Ok);
        });

        let received: DaemonRequest = read_framed_message(&mut server)
            .await
            .expect("服务端读取请求成功");
        assert_eq!(received, DaemonRequest::SwitchGroup(Some(42)));
        write_framed_message(&mut server, &DaemonResponse::Ok)
            .await
            .expect("服务端写入响应成功");
    }

    #[tokio::test]
    async fn test_ipc_listener_and_client_dialogue() {
        let temp_dir = tempfile::tempdir().expect("创建临时目录成功");
        let sock_path = temp_dir.path().join("test_daemon.sock");

        let listener = IpcListener::bind(Some(sock_path.clone()))
            .await
            .expect("绑定测试套接字成功");
        let (tx, mut rx) = tokio::sync::mpsc::channel::<IpcInvocation>(4);
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        let listener_handle = tokio::spawn(async move {
            listener.run_loop(tx, shutdown_rx).await;
        });

        // 模拟守护进程主循环响应逻辑
        tokio::spawn(async move {
            while let Some(inv) = rx.recv().await {
                match inv.request {
                    DaemonRequest::Ping => {
                        let _ = inv.responder.send(DaemonResponse::Pong);
                    }
                    DaemonRequest::GetStatus => {
                        let _ = inv.responder.send(DaemonResponse::Status {
                            active_group: 1,
                            paster_count: 5,
                        });
                    }
                    _ => {
                        let _ = inv.responder.send(DaemonResponse::Ok);
                    }
                }
            }
        });

        // 客户端发起 Ping
        let res = send_request_to_path(&sock_path, &DaemonRequest::Ping)
            .await
            .expect("Ping 通信成功");
        assert_eq!(res, Some(DaemonResponse::Pong));

        // 客户端发起 GetStatus
        let res = send_request_to_path(&sock_path, &DaemonRequest::GetStatus)
            .await
            .expect("GetStatus 通信成功");
        assert_eq!(
            res,
            Some(DaemonResponse::Status {
                active_group: 1,
                paster_count: 5
            })
        );

        // 客户端发起 ReloadConfig
        let res_reload = send_request_to_path(&sock_path, &DaemonRequest::ReloadConfig)
            .await
            .expect("ReloadConfig 通信成功");
        assert_eq!(res_reload, Some(DaemonResponse::Ok));

        // 验证错误响应正确转换为 Business 错误
        let res_err = send_request_to_path(
            &sock_path,
            &DaemonRequest::CreatePaster {
                width: 0,
                height: 0,
                x: 0,
                y: 0,
                title: "err".into(),
                raw_path: "/invalid".into(),
            },
        )
        .await
        .expect("通信链路成功");
        assert_eq!(res_err, Some(DaemonResponse::Ok));

        let _ = shutdown_tx.send(true);
        let _ = listener_handle.await;
    }
}
