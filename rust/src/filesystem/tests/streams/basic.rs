use super::*;

#[test]
fn independent_vfs_instances_never_share_stream_identifiers() {
    let left = FsServer::new();
    let right = FsServer::new();
    let left_stream = left
        .serve(
            &Memory { writable: true },
            &request("vfs.openRead", json!({"nodeId":"a"})),
        )
        .unwrap()
        .unwrap();
    let right_stream = right
        .serve(
            &Memory { writable: true },
            &request("vfs.openRead", json!({"nodeId":"a"})),
        )
        .unwrap()
        .unwrap();
    assert_ne!(left_stream["streamId"], right_stream["streamId"]);
    assert!(
        right
            .serve(
                &Memory { writable: true },
                &request(
                    "vfs.readStream",
                    json!({
                        "streamId":left_stream["streamId"], "offset":0, "maxBytes":1
                    })
                )
            )
            .is_err()
    );
}

#[test]
fn read_stream_honours_offsets_and_rejects_backward_seeks() {
    let server = FsServer::new();
    let opened = server
        .serve(
            &Memory { writable: true },
            &request("vfs.openRead", json!({"nodeId": "a"})),
        )
        .unwrap()
        .unwrap();
    assert_eq!(opened["size"], 4);
    let id = opened["streamId"].as_str().unwrap();
    let first = server
        .serve(
            &Memory { writable: true },
            &request(
                "vfs.readStream",
                json!({"streamId": id, "offset": 0, "maxBytes": 2}),
            ),
        )
        .unwrap()
        .unwrap();
    assert_eq!(first["base64"], "YWI=");
    let second = server
        .serve(
            &Memory { writable: true },
            &request(
                "vfs.readStream",
                json!({"streamId": id, "offset": 2, "maxBytes": 2}),
            ),
        )
        .unwrap()
        .unwrap();
    assert_eq!(second["base64"], "Y2Q=");
    assert!(
        server
            .serve(
                &Memory { writable: true },
                &request(
                    "vfs.readStream",
                    json!({"streamId": id, "offset": 1, "maxBytes": 2}),
                ),
            )
            .is_err()
    );
    server
        .serve(
            &Memory { writable: true },
            &request("vfs.closeRead", json!({"streamId": id})),
        )
        .unwrap()
        .unwrap();
}

#[test]
fn writes_commit_through_the_provider_callback() {
    let server = FsServer::new();
    let provider = Memory { writable: true };
    let opened = server
        .serve(
            &provider,
            &request("vfs.beginWrite", json!({"nodeId": "a", "ifRevision": "r1"})),
        )
        .unwrap()
        .unwrap();
    let id = opened["streamId"].as_str().unwrap().to_string();
    server
        .serve(
            &provider,
            &request(
                "vfs.writeStream",
                json!({"streamId": id, "offset": 0, "base64": "YWJjZA=="}),
            ),
        )
        .unwrap();
    let committed = server
        .serve(&provider, &request("vfs.commit", json!({"streamId": id})))
        .unwrap()
        .unwrap();
    assert_eq!(committed["revision"], "r-r1");

    let reopened = server
        .serve(
            &provider,
            &request("vfs.beginWrite", json!({"nodeId": "a"})),
        )
        .unwrap()
        .unwrap();
    let id = reopened["streamId"].as_str().unwrap().to_string();
    server
        .serve(&provider, &request("vfs.abort", json!({"streamId": id})))
        .unwrap();
    assert!(
        server
            .serve(&provider, &request("vfs.commit", json!({"streamId": id})))
            .is_err()
    );
}

#[test]
fn provider_without_write_capability_cannot_begin_a_write() {
    let server = FsServer::new();
    let error = server
        .serve(
            &ReadOnly,
            &request("vfs.beginWrite", json!({"nodeId": "a"})),
        )
        .unwrap_err();
    assert_eq!(error.data.unwrap()["operation"], "write");
}
