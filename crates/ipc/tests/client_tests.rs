use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use ipc::{ClientError, IpcClient, IpcRequest, IpcResponse, IpcServer, ServerError};

static SOCKET_COUNTER: AtomicU64 = AtomicU64::new(0);

#[tokio::test]
async fn sends_request_and_receives_response() {
    let socket_path = temporary_socket_path();

    let server = IpcServer::bind(&socket_path).expect("failed to bind server");

    let server_task = tokio::spawn(async move {
        let mut connection = server.accept().await.expect("accept failed");

        let request = connection.read_request().await.expect("request failed");

        assert_eq!(request, IpcRequest::Ping);

        connection
            .send_response(&IpcResponse::Pong)
            .await
            .expect("response failed");
    });

    let mut client = IpcClient::connect(&socket_path)
        .await
        .expect("connection failed");

    let response = client
        .send(&IpcRequest::Ping)
        .await
        .expect("request failed");

    assert_eq!(response, IpcResponse::Pong);

    server_task.await.expect("server task failed");
}

#[tokio::test]
async fn removes_socket_when_server_is_dropped() {
    let socket_path = temporary_socket_path();

    {
        let _server = IpcServer::bind(&socket_path).expect("server bind failed");

        assert!(socket_path.exists());
    }

    assert!(!socket_path.exists());
}

fn temporary_socket_path() -> PathBuf {
    let counter = SOCKET_COUNTER.fetch_add(1, Ordering::Relaxed);

    std::env::temp_dir().join(format!(
        "pookie-paste-ipc-test-{}-{}.sock",
        std::process::id(),
        counter,
    ))
}

#[tokio::test]
async fn supports_multiple_requests_on_same_connection() {
    let socket_path = temporary_socket_path();

    let server = IpcServer::bind(&socket_path).expect("server bind failed");

    let server_task = tokio::spawn(async move {
        let mut connection = server.accept().await.expect("accept failed");

        for _ in 0..2 {
            let request = connection
                .read_request()
                .await
                .expect("request read failed");

            assert_eq!(request, IpcRequest::Ping,);

            connection
                .send_response(&IpcResponse::Pong)
                .await
                .expect("response write failed");
        }
    });

    let mut client = IpcClient::connect(&socket_path)
        .await
        .expect("client connection failed");

    let first = client
        .send(&IpcRequest::Ping)
        .await
        .expect("first request failed");

    let second = client
        .send(&IpcRequest::Ping)
        .await
        .expect("second request failed");

    assert_eq!(first, IpcResponse::Pong);
    assert_eq!(second, IpcResponse::Pong);

    server_task.await.expect("server task failed");
}

#[tokio::test]
async fn reports_when_server_closes_connection_without_response() {
    let socket_path = temporary_socket_path();

    let server = IpcServer::bind(&socket_path).expect("server bind failed");

    let server_task = tokio::spawn(async move {
        let mut connection = server.accept().await.expect("accept failed");

        let request = connection
            .read_request()
            .await
            .expect("request read failed");

        assert_eq!(request, IpcRequest::Ping,);

        // Intentionally close the connection
        // without sending a response.
        drop(connection);
    });

    let mut client = IpcClient::connect(&socket_path)
        .await
        .expect("client connection failed");

    let result = client.send(&IpcRequest::Ping).await;

    assert!(matches!(result, Err(ClientError::ConnectionClosed)));

    server_task.await.expect("server task failed");
}

#[tokio::test]
async fn rejects_malformed_request() {
    let socket_path = temporary_socket_path();

    let server = IpcServer::bind(&socket_path).expect("server bind failed");

    let server_task = tokio::spawn(async move {
        let mut connection = server.accept().await.expect("accept failed");

        let result = connection.read_request().await;

        assert!(matches!(result, Err(ServerError::Codec(_))));
    });

    let mut stream = tokio::net::UnixStream::connect(&socket_path)
        .await
        .expect("connection failed");

    use tokio::io::AsyncWriteExt;

    stream
        .write_all(b"this is not json\n")
        .await
        .expect("write failed");

    server_task.await.expect("server task failed");
}
