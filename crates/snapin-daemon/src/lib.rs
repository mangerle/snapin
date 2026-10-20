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
    get_socket_path, DaemonRequest, DaemonResponse, IpcInvocation, MAX_IPC_FRAME_SIZE,
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

    /// 验证停机信号可让监听循环立即收手，并在退出时清理套接字文件。
    ///
    /// 守护进程作为 systemd user 服务常驻，若停机路径挂起或残留套接字，
    /// 下一次启动会被单实例探测误判为"已有实例存活"，因此两者都必须可断言。
    #[tokio::test]
    async fn test_graceful_shutdown_stops_loop_and_removes_socket() {
        let temp_dir = tempfile::tempdir().expect("创建临时目录成功");
        let sock_path = temp_dir.path().join("shutdown.sock");

        let listener = IpcListener::bind(Some(sock_path.clone()))
            .await
            .expect("绑定测试套接字成功");
        assert!(sock_path.exists(), "绑定后套接字文件应存在");

        let (tx, _rx) = tokio::sync::mpsc::channel::<IpcInvocation>(4);
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let listener_handle = tokio::spawn(async move { listener.run_loop(tx, shutdown_rx).await });

        shutdown_tx.send(true).expect("发送停机信号给监听循环成功");

        tokio::time::timeout(std::time::Duration::from_secs(5), listener_handle)
            .await
            .expect("监听循环应在停机信号后退出")
            .expect("监听任务不应 panic");

        assert!(!sock_path.exists(), "退出时必须清理 IPC 套接字文件");
    }

    /// 验证业务分发通道已关闭时，接入连接会收到明确的中文错误响应而非让客户端挂起等待超时。
    #[tokio::test]
    async fn test_client_receives_error_when_dispatcher_closed() {
        let temp_dir = tempfile::tempdir().expect("创建临时目录成功");
        let sock_path = temp_dir.path().join("closed-dispatcher.sock");

        let listener = IpcListener::bind(Some(sock_path.clone()))
            .await
            .expect("绑定测试套接字成功");

        let (tx, rx) = tokio::sync::mpsc::channel::<IpcInvocation>(4);
        drop(rx);
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let listener_handle = tokio::spawn(async move { listener.run_loop(tx, shutdown_rx).await });

        let response = send_request_to_path(&sock_path, &DaemonRequest::Ping)
            .await
            .expect("Ping 通信链路成功");
        match response {
            Some(DaemonResponse::Error(message)) => {
                assert!(
                    message.contains("主循环已关闭"),
                    "错误响应应说明分发通道已关闭，实际: {message}"
                );
            }
            other => panic!("分发通道关闭后应返回错误响应，实际: {other:?}"),
        }

        _shutdown_tx.send(true).expect("发送停机信号成功");
        let _ = listener_handle.await;
    }

    /// 验证套接字路径的单实例语义：残留文件自动回收后可重新绑定，已有存活实例应答时拒绝重复绑定。
    #[tokio::test]
    async fn test_bind_rejects_live_instance_and_recovers_stale_socket() {
        let temp_dir = tempfile::tempdir().expect("创建临时目录成功");
        let sock_path = temp_dir.path().join("single-instance.sock");

        // 1. 崩溃残留的占位文件无人应答，必须被识别为陈旧套接字并自动清理
        std::fs::write(&sock_path, b"not a socket").expect("写入残留占位文件成功");
        let recovered = IpcListener::bind(Some(sock_path.clone()))
            .await
            .expect("残留套接字应被自动清理并成功重新绑定");
        assert!(sock_path.exists(), "重新绑定后套接字应就绪");
        drop(recovered);
        assert!(!sock_path.exists(), "监听器析构后应移除套接字文件");

        // 2. 已有实例真实在线并应答 Ping 时，第二次绑定必须被拒绝，避免双实例热键争抢
        let live = IpcListener::bind(Some(sock_path.clone()))
            .await
            .expect("首次绑定在线实例成功");
        let (tx, mut rx) = tokio::sync::mpsc::channel::<IpcInvocation>(4);
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let live_handle = tokio::spawn(async move { live.run_loop(tx, shutdown_rx).await });
        tokio::spawn(async move {
            while let Some(inv) = rx.recv().await {
                let response = match inv.request {
                    DaemonRequest::Ping => DaemonResponse::Pong,
                    other => DaemonResponse::Error(format!("测试实例不处理 {other:?}")),
                };
                let _ = inv.responder.send(response);
            }
        });

        let probe = send_request_to_path(&sock_path, &DaemonRequest::Ping)
            .await
            .expect("在线实例 Ping 探测通信成功");
        assert_eq!(probe, Some(DaemonResponse::Pong), "在线实例应应答 Ping");

        match IpcListener::bind(Some(sock_path.clone())).await {
            Err(DaemonError::Socket { message, .. }) => assert!(
                message.contains("已有另一个"),
                "重复绑定应提示已有实例存活，实际: {message}"
            ),
            Err(other) => panic!("存活实例占用时应返回套接字占用错误，实际: {other}"),
            Ok(_) => panic!("存活实例占用时第二次绑定必须失败"),
        }

        _shutdown_tx.send(true).expect("发送停机信号成功");
        let _ = live_handle.await;
    }
}
