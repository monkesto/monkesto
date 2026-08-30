use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::journal::file::{FileId, ObjectStore};
use crate::journal::layout::layout;
use crate::journal::{JournalError, JournalId};
use crate::monkesto_error::{MonkestoError, OrRedirect, UrlError};
use crate::{BackendType, StateType};
use aws_sdk_s3::presigning::PresigningConfig;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum_login::AuthSession;
use maud::{Markup, PreEscaped, html};
use std::str::FromStr;
use std::time::Duration;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

pub async fn file_list_page(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    let user = get_user(session)?;
    let user_authority = Authority::Direct(Actor::User(user.id));
    let journal_id_res = JournalId::from_str(&id);

    const UPLOAD_JS: &str = include_str!("upload.js");

    let content = html! {
        p id="statusText" class="text-sm font-medium text-gray-600 mt-2 text-center" {

        }

        div id="progressContainer" class="hidden w-full bg-gray-200 rounded-full h-4 mt-5 overflow-hidden" {
            div id="progressBar" class="bg-emerald-500 h-full w-0 transition-all duration-150 ease-out rounded-full" {

            }
        }

        script {
            (PreEscaped(UPLOAD_JS))
        }

        div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4" {
            @if let Ok(journal_id) = journal_id_res {
                @match state.journal_service.list_journal_files(journal_id, user_authority).await {
                    Ok(files) => {
                        @for (file, uploader, upload_timestamp) in files {
                            a
                            href=(format! ("/journal/{}/file/{}", journal_id, file.id))
                            class="self-start p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors" {
                                h3 class="text-lg font-semibold text-gray-900 dark:text-white" {
                                    (file.name)
                                }

                                div class="mt-2 text-sm text-gray-600 dark:text-gray-400" {
                                    "Uploaded by "

                                    @match uploader.actor() {
                                        Actor::System => {"System"},
                                        Actor::Anonymous => {"Anonymous"},
                                        Actor::User(uploader_id) => {
                                             @match state.authn_service.fetch_user(*uploader_id).await {
                                                Ok(user) => (user.email.to_string()),

                                                Err(e) => (format!("failed to fetch uploader email: {:?}", e)),
                                            }
                                        }
                                    }

                                    " on "

                                    (upload_timestamp.with_timezone(&chrono_tz::America::Chicago).format("%Y-%m-%d %H:%M:%S %Z"))

                                }
                            }
                        }
                    }

                    Err(e) => {
                        div class="flex justify-center items-center h-full" {
                            p class="text-gray-500 dark:text-gray-400" {
                                (format!("Failed to fetch journals: {:?}", e))
                            }
                        }
                    }
                }

                button
                    onclick="upload()"
                    class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500" {
                        "Upload File"
                    }
            }
            @else {
                div class="flex justify-center items-center h-full" {
                    p class="text-gray-500 dark:text-gray-400" {
                        "Invalid journal Id"
                    }
                }
            }
        }
        @if let Some(error_str) = err.err {
                (MonkestoError::decode(&error_str))
            }
    };

    let wrapped_content = html! {
        div class="flex flex-col gap-6 mx-auto w-full max-w-4xl" {
            (content)
        }
    };

    let journal_name = if let Ok(id) = journal_id_res {
        state
            .journal_service
            .get_journal(id, user_authority)
            .await
            .map(|(j, _, _)| j.name.to_string())
            .unwrap_or_else(|e| format!("failed to fetch the journal name: {e}"))
    } else {
        "invalid journal id".to_string()
    };

    Ok(layout(
        Some(&journal_name),
        true,
        Some(&id),
        wrapped_content,
    ))
}

pub async fn download_file(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path((journal_id, file_id)): Path<(String, String)>,
) -> Result<Response, Redirect> {
    let callback_url = &format!("/journal/{}/file", journal_id);

    let user = get_user(session)?;
    let user_authority = Authority::Direct(Actor::User(user.id));
    let journal_id = JournalId::from_str(&journal_id).or_redirect(callback_url)?;
    let file_id = FileId::from_str(&file_id).or_redirect(callback_url)?;

    let file = state
        .journal_service
        .get_file(file_id, journal_id, user_authority)
        .await
        .or_redirect(callback_url)?;

    let file_key = file.key();

    match state.journal_service.object_store {
        ObjectStore::S3 {
            s3_client,
            bucket_name,
        } => {
            let presign_ttl = Duration::from_secs(30);
            let presigning_config =
                PresigningConfig::expires_in(presign_ttl).expect("valid presign ttl");

            let presigned_req = s3_client
                .get_object()
                .bucket(bucket_name)
                .key(&file_key)
                .response_content_disposition(format!("filename=\"{}\"", file.name))
                .presigned(presigning_config)
                .await
                .map_err(JournalError::from)
                .or_redirect(callback_url)?;

            Ok(Redirect::temporary(presigned_req.uri()).into_response())
        }
        ObjectStore::Local { storage_directory } => {
            let disk_file = File::open(storage_directory.join(file_key))
                .await
                .map_err(|e| JournalError::S3(e.to_string()))
                .or_redirect(callback_url)?;

            let stream = ReaderStream::new(disk_file);

            let body = Body::from_stream(stream);

            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                "application/octet-stream"
                    .parse()
                    .expect("valid header value"),
            );
            headers.insert(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", file.name)
                    .parse()
                    .expect("valid header value"),
            );

            Ok((headers, body).into_response())
        }
    }
}
