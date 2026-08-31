use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::AsyncWriteExt;

use ipc::{
    ClientError, IpcClient, IpcRequest, IpcResponse, IpcServer, MAX_FRAME_SIZE, ServerError,
};

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

#[tokio::test]
async fn rejects_oversized_request_frame() {
    let socket_path = temporary_socket_path();

    let server = IpcServer::bind(&socket_path).expect("server bind failed");

    let server_task = tokio::spawn(async move {
        let mut connection = server.accept().await.expect("accept failed");

        connection.read_request().await
    });

    let mut stream = tokio::net::UnixStream::connect(&socket_path)
        .await
        .expect("connection failed");

    let oversized = vec![b'a'; MAX_FRAME_SIZE + 1];

    stream
        .write_all(&oversized)
        .await
        .expect("oversized write failed");

    stream.shutdown().await.expect("stream shutdown failed");

    let result = server_task.await.expect("server task failed");

    assert!(
        matches!(result, Err(ServerError::FrameTooLarge { .. })),
        "expected FrameTooLarge, got: {result:?}"
    );
}

#[tokio::test]
async fn server_accepts_healthy_client_after_malformed_client() {
    let socket_path = temporary_socket_path();

    let server = IpcServer::bind(&socket_path).expect("server bind failed");

    let server_task = tokio::spawn(async move {
        // First connection: bad client.
        let mut bad_connection = server.accept().await.expect("bad client accept failed");

        let bad_result = bad_connection.read_request().await;

        assert!(
            matches!(bad_result, Err(ServerError::Codec(_))),
            "expected malformed request to return codec error"
        );

        // Second connection: healthy client.
        let mut healthy_connection = server.accept().await.expect("healthy client accept failed");

        let request = healthy_connection
            .read_request()
            .await
            .expect("healthy request read failed");

        assert_eq!(request, IpcRequest::Ping);

        healthy_connection
            .send_response(&IpcResponse::Pong)
            .await
            .expect("healthy response failed");
    });

    // Bad client sends malformed JSON.
    {
        let mut stream = tokio::net::UnixStream::connect(&socket_path)
            .await
            .expect("bad client connection failed");

        stream
            .write_all(b"broken-json\n")
            .await
            .expect("bad client write failed");
    }

    // Healthy client connects after bad client.
    let mut client = IpcClient::connect(&socket_path)
        .await
        .expect("healthy client connection failed");

    let response = client
        .send(&IpcRequest::Ping)
        .await
        .expect("healthy request failed");

    assert_eq!(response, IpcResponse::Pong);

    server_task.await.expect("server task failed");
}

#[tokio::test]
async fn server_accepts_healthy_client_after_oversized_client() {
    let socket_path = temporary_socket_path();

    let server = IpcServer::bind(&socket_path).expect("server bind failed");

    let server_task = tokio::spawn(async move {
        // First connection: oversized client.
        let mut bad_connection = server
            .accept()
            .await
            .expect("oversized client accept failed");

        let bad_result = bad_connection.read_request().await;

        assert!(
            matches!(bad_result, Err(ServerError::FrameTooLarge { .. })),
            "expected oversized request to return FrameTooLarge"
        );

        // Second connection: healthy client.
        let mut healthy_connection = server.accept().await.expect("healthy client accept failed");

        let request = healthy_connection
            .read_request()
            .await
            .expect("healthy request read failed");

        assert_eq!(request, IpcRequest::Ping);

        healthy_connection
            .send_response(&IpcResponse::Pong)
            .await
            .expect("healthy response failed");
    });

    // Oversized bad client.
    {
        let mut stream = tokio::net::UnixStream::connect(&socket_path)
            .await
            .expect("oversized client connection failed");

        let oversized = vec![b'x'; MAX_FRAME_SIZE + 1];

        stream
            .write_all(&oversized)
            .await
            .expect("oversized write failed");

        stream
            .shutdown()
            .await
            .expect("oversized client shutdown failed");
    }

    // Healthy client connects afterward.
    let mut client = IpcClient::connect(&socket_path)
        .await
        .expect("healthy client connection failed");

    let response = client
        .send(&IpcRequest::Ping)
        .await
        .expect("healthy request failed");

    assert_eq!(response, IpcResponse::Pong);

    server_task.await.expect("server task failed");
}

#[tokio::test]
async fn replaces_stale_socket() {
    let path = temporary_socket_path();

    {
        let listener =
            std::os::unix::net::UnixListener::bind(&path).expect("failed to create stale socket");

        drop(listener);
    }

    assert!(path.exists(), "stale socket should still exist");

    let server = IpcServer::bind(&path).expect("server should replace stale socket");

    assert!(path.exists());

    drop(server);

    assert!(
        !path.exists(),
        "socket should be removed when server is dropped"
    );
}

#[tokio::test]
async fn refuses_to_replace_live_server_socket() {
    let path = temporary_socket_path();

    let first = IpcServer::bind(&path).expect("first bind failed");

    let second = IpcServer::bind(&path);

    assert!(
        matches!(second, Err(ServerError::AlreadyRunning { .. })),
        "second server should detect existing live server"
    );

    assert!(path.exists(), "live server socket must remain intact");

    drop(first);

    assert!(!path.exists());
}

#[tokio::test]
async fn refuses_to_delete_non_socket_file() {
    let path = temporary_socket_path();

    std::fs::write(&path, b"do not delete").expect("file creation failed");

    let result = IpcServer::bind(&path);

    assert!(
        result.is_err(),
        "bind should fail when path is a normal file"
    );

    assert!(path.exists(), "normal file must not be deleted");

    let contents = std::fs::read(&path).expect("failed to read protected file");

    assert_eq!(contents, b"do not delete");

    std::fs::remove_file(&path).expect("test cleanup failed");
}
