# War Room Phase 2 (Chat + Realtime) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship war room chat (list/create/edit/delete + `@mention` notifications) and the realtime relay: api-rs publishes `war-room:events` to Redis after writes; `apps/live` subscribes once and fans out to per-room WebSockets with typing/presence.

**Architecture:** REST persists in Postgres first (api-rs), then a best-effort `publish_war_room_event` pushes `{room_id, kind, data}` to Redis channel `war-room:events`. `apps/live` runs a `WarRoomRelay` singleton: in-memory `Map<roomId, Set<socket>>`, one duplicated ioredis subscriber, fan-out to room sockets. Ephemeral typing/presence flows through the same channel; auth reuses `handleAuthentication` plus a REST `GET war-rooms/:pk/` access check with the handshake cookie.

**Tech Stack:** Rust (axum, sqlx, redis-rs), Redis/Valkey pub/sub, TypeScript (express-ws, ioredis, ws, vitest) in `apps/live`.

**Preconditions:** Phase 1 sudah merge di branch ini (commit `e20e3e37d`..`7e2233589`). DB dev + `plane-redis` (localhost:6379) hidup. Tidak ada perubahan `apps/web` di fase ini.

---

## File Structure

| File                                               | Aksi   | Tanggung jawab                                                               |
| -------------------------------------------------- | ------ | ---------------------------------------------------------------------------- |
| `apps/api-rs/crates/api/src/routes/war_room.rs`    | modify | `parse_mentions`, message rows/handler, publisher, retrofit activity publish |
| `apps/api-rs/crates/api/src/main.rs`               | modify | registrasi route `messages`                                                  |
| `apps/api-rs/crates/api/Cargo.toml`                | modify | dev-dep `futures` untuk test subscriber Redis                                |
| `apps/api-rs/crates/api/tests/war_room_test.rs`    | modify | test messages + publish + cleanup notifications                              |
| `apps/live/src/types/war-room.ts`                  | create | tipe relay/handshake                                                         |
| `apps/live/src/types/index.ts`                     | modify | re-export tipe war room                                                      |
| `apps/live/src/lib/war-room-auth.ts`               | create | parse handshake WS (token + cookie + query)                                  |
| `apps/live/src/services/war-room.service.ts`       | create | REST cek akses room                                                          |
| `apps/live/src/services/war-room-relay.service.ts` | create | registry room + subscriber Redis + fan-out                                   |
| `apps/live/src/controllers/war-room.controller.ts` | create | endpoint WS `/war-rooms/:roomId`                                             |
| `apps/live/src/controllers/index.ts`               | modify | daftarkan controller                                                         |
| `apps/live/src/server.ts`                          | modify | init/destroy relay                                                           |
| `apps/live/tests/lib/war-room-auth.test.ts`        | create | unit test parser handshake                                                   |
| `apps/live/tests/services/war-room-relay.test.ts`  | create | unit test fan-out/registry                                                   |

---

### Task 1: Mention tokenizer (pure helper + unit test)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/war_room.rs`

- [ ] **Step 1: Tulis test yang gagal**

Tambahkan dua test berikut ke dalam `mod tests` di `war_room.rs` (setelah test `active_status_check`):

```rust
    #[test]
    fn mentions_parse_valid_uuids_only_once() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let body = format!(
            "hey @{{{a}}} and @{{{b}}} and @{{{a}}} bad @{{nope}} tail @{{"
        );
        assert_eq!(parse_mentions(&body), vec![a, b]);
    }

    #[test]
    fn mentions_parse_empty_when_none() {
        assert!(parse_mentions("no mentions here").is_empty());
        assert!(parse_mentions("").is_empty());
    }
```

- [ ] **Step 2: Run test untuk memastikan gagal**

Run: `cargo test -p api --lib war_room::tests` (dari `apps/api-rs`)
Expected: FAIL compile — `parse_mentions` belum ada.

- [ ] **Step 3: Implementasi `parse_mentions`**

Tambahkan di `war_room.rs`, tepat sebelum `runbook_template` (setelah helper `status_transition_allowed`):

```rust
/// Extract unique user ids from `@{uuid}` tokens. Invalid tokens are ignored.
/// Mentions are stored raw here; the handler filters them to workspace members.
pub fn parse_mentions(body: &str) -> Vec<Uuid> {
    let mut mentions: Vec<Uuid> = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("@{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else {
            break;
        };
        if let Ok(user_id) = Uuid::parse_str(&after[..end]) {
            if !mentions.contains(&user_id) {
                mentions.push(user_id);
            }
        }
        rest = &after[end + 1..];
    }
    mentions
}
```

- [ ] **Step 4: Run test untuk memastikan lulus**

Run: `cargo test -p api --lib war_room::tests`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/war_room.rs
git commit -m "feat(api-rs): war room mention token parser"
```

---

### Task 2: Message row, serializer, `messages_list`

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/war_room.rs`

- [ ] **Step 1: Tambahkan row type + serializer + fetch helpers**

Tambahkan tepat sebelum `#[cfg(test)] mod tests` (setelah `events_list`):

```rust
// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WarRoomMessageRow {
    pub id: Uuid,
    pub war_room_id: Uuid,
    pub author_id: Option<Uuid>,
    pub body: String,
    pub mentions: Value,
    pub edited_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

const MESSAGE_SELECT: &str = "SELECT m.id, m.war_room_id, m.author_id, m.body, m.mentions, \
    m.edited_at, m.created_at, u.display_name, \
    CASE WHEN u.avatar_asset_id IS NOT NULL \
      THEN '/api/assets/v2/static/' || u.avatar_asset_id::text || '/' ELSE u.avatar END AS avatar_url \
    FROM war_room_messages m LEFT JOIN users u ON u.id = m.author_id";

pub fn message_json(r: &WarRoomMessageRow) -> Value {
    serde_json::json!({
        "id": r.id,
        "war_room_id": r.war_room_id,
        "author_id": r.author_id,
        "author": r.author_id.map(|id| serde_json::json!({
            "id": id, "display_name": r.display_name, "avatar_url": r.avatar_url,
        })),
        "body": r.body,
        "mentions": r.mentions,
        "edited_at": r.edited_at,
        "created_at": r.created_at,
    })
}

async fn fetch_message(
    pool: &PgPool,
    room_id: Uuid,
    message_id: Uuid,
) -> Result<Option<WarRoomMessageRow>, sqlx::Error> {
    sqlx::query_as(&format!(
        "{MESSAGE_SELECT} WHERE m.id = $1 AND m.war_room_id = $2 AND m.deleted_at IS NULL"
    ))
    .bind(message_id)
    .bind(room_id)
    .fetch_optional(pool)
    .await
}

#[derive(Debug, Deserialize)]
pub struct MessagesParams {
    pub before_id: Option<Uuid>,
    pub limit: Option<i64>,
}

/// Chat pages are returned oldest → newest inside the page (client renders
/// directly); `before_id` walks backwards with the first id of the page.
pub async fn messages_list(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Query(params): Query<MessagesParams>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    if !gate_member(&st.pool, auth.0, &slug, project_id).await? {
        return Ok(deny());
    }
    let Some(_room) = fetch_room(&st.pool, project_id, pk).await? else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "War room not found"})),
        ));
    };
    let cursor: Option<(chrono::DateTime<chrono::Utc>, Uuid)> = match params.before_id {
        Some(before_id) => {
            sqlx::query_as(
                "SELECT created_at, id FROM war_room_messages \
                 WHERE id = $1 AND war_room_id = $2 AND deleted_at IS NULL",
            )
            .bind(before_id)
            .bind(pk)
            .fetch_optional(&st.pool)
            .await?
        }
        None => None,
    };
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let mut rows: Vec<WarRoomMessageRow> = sqlx::query_as(&format!(
        "{MESSAGE_SELECT} WHERE m.war_room_id = $1 \
         AND ($2::timestamptz IS NULL OR (m.created_at, m.id) < ($2::timestamptz, $3::uuid)) \
         AND m.deleted_at IS NULL \
         ORDER BY m.created_at DESC, m.id DESC LIMIT $4"
    ))
    .bind(pk)
    .bind(cursor.as_ref().map(|c| c.0))
    .bind(cursor.as_ref().map(|c| c.1))
    .bind(limit)
    .fetch_all(&st.pool)
    .await?;
    rows.reverse();
    Ok((
        StatusCode::OK,
        Json(Value::Array(rows.iter().map(message_json).collect())),
    ))
}
```

- [ ] **Step 2: Verifikasi compile**

Run: `cargo check -p api`
Expected: LULUS (warning dead code `message_json`/`fetch_message` boleh muncul sampai Task 4–5).

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/war_room.rs
git commit -m "feat(api-rs): war room message row, serializer, list handler"
```

---

### Task 3: Redis publisher + retrofit activity publish ke semua handler fase 1

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/war_room.rs`

- [ ] **Step 1: Tambahkan import redis**

Di blok import atas `war_room.rs`, tambahkan setelah `use serde_json::Value;`:

```rust
use redis::AsyncCommands;
```

- [ ] **Step 2: Ubah `record_event` mengembalikan row + tambah publisher & `record_and_publish`**

Ganti seluruh blok `pub async fn record_event(...)` (saat ini berakhir `Ok(())`) dengan:

```rust
pub async fn record_event(
    pool: &PgPool,
    room: &WarRoomRow,
    actor: Uuid,
    event_type: &str,
    payload: Value,
) -> Result<WarRoomEventRow, sqlx::Error> {
    sqlx::query_as(
        "INSERT INTO war_room_events (id, workspace_id, project_id, war_room_id, actor_id, \
         event_type, payload, created_at) VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, $6, now()) \
         RETURNING id, actor_id, event_type, payload, created_at",
    )
    .bind(room.workspace_id)
    .bind(room.project_id)
    .bind(room.id)
    .bind(actor)
    .bind(event_type)
    .bind(payload)
    .fetch_one(pool)
    .await
}

/// Realtime channel consumed by `apps/live` (`war-room-relay.service.ts`).
pub const WAR_ROOM_CHANNEL: &str = "war-room:events";

/// Best-effort realtime publish; a Redis hiccup must never fail the write.
pub async fn publish_war_room_event(st: &AppState, room_id: Uuid, kind: &str, data: Value) {
    let payload = serde_json::json!({ "room_id": room_id, "kind": kind, "data": data });
    match st.redis_client().await {
        Ok(mut conn) => {
            let result: Result<i64, redis::RedisError> =
                conn.publish(WAR_ROOM_CHANNEL, payload.to_string()).await;
            if let Err(error) = result {
                tracing::warn!(room_id=%room_id, kind, error=%error, "war room publish failed");
            }
        }
        Err(error) => {
            tracing::warn!(room_id=%room_id, kind, error=%error, "war room redis unavailable");
        }
    }
}

/// Insert an activity row, then publish `activity.created` plus a
/// `room.changed` hint with the affected sections (`reasons` may be empty).
pub async fn record_and_publish(
    st: &AppState,
    room: &WarRoomRow,
    actor: Uuid,
    event_type: &str,
    payload: Value,
    reasons: &[&str],
) -> Result<WarRoomEventRow, sqlx::Error> {
    let row = record_event(&st.pool, room, actor, event_type, payload).await?;
    publish_war_room_event(st, room.id, "activity.created", event_json(&row)).await;
    if !reasons.is_empty() {
        publish_war_room_event(
            st,
            room.id,
            "room.changed",
            serde_json::json!({ "reasons": reasons }),
        )
        .await;
    }
    Ok(row)
}
```

- [ ] **Step 3: Retrofit call site `patch`**

Ganti blok dari `if severity != current.severity {` sampai penutup `if status != current.status { ... }` dengan:

```rust
    if severity != current.severity {
        record_and_publish(
            &st,
            &current,
            auth.0,
            "room.severity_changed",
            serde_json::json!({ "from": current.severity, "to": severity }),
            &["severity"],
        )
        .await?;
    }
    if status != current.status {
        record_and_publish(
            &st,
            &current,
            auth.0,
            "room.status_changed",
            serde_json::json!({ "from": current.status, "to": status }),
            &["status"],
        )
        .await?;
        let specific = match status.as_str() {
            "resolved" => Some("room.resolved"),
            "active" if current.status == "resolved" => Some("room.reopened"),
            "archived" => Some("room.archived"),
            _ => None,
        };
        if let Some(event_type) = specific {
            record_and_publish(&st, &current, auth.0, event_type, serde_json::json!({}), &[])
                .await?;
        }
    }
    let mut detail_reasons: Vec<&str> = Vec::new();
    if name != current.name || description_html != current.description_html {
        detail_reasons.push("details");
    }
    if notes_html != current.notes_html {
        detail_reasons.push("notes");
    }
    if !detail_reasons.is_empty() {
        publish_war_room_event(
            &st,
            current.id,
            "room.changed",
            serde_json::json!({ "reasons": detail_reasons }),
        )
        .await;
    }
```

- [ ] **Step 4: Retrofit call site `services_create`**

Ganti blok `if linked > 0 { record_event(...) .await?; }` dengan:

```rust
    if linked > 0 {
        record_and_publish(
            &st,
            &room,
            auth.0,
            "service.linked",
            serde_json::json!({ "service_ids": body.service_ids }),
            &["links"],
        )
        .await?;
    }
```

- [ ] **Step 5: Retrofit `services_destroy`**

Ganti blok `if result.rows_affected() > 0 { record_event(...) .await?; }` dengan:

```rust
    if result.rows_affected() > 0 {
        record_and_publish(
            &st,
            &room,
            auth.0,
            "service.unlinked",
            serde_json::json!({ "service_id": service_id }),
            &["links"],
        )
        .await?;
    }
```

- [ ] **Step 6: Retrofit `issues_create`**

Ganti blok `if linked > 0 { record_event(...) .await?; }` (yang memakai `"issue.linked"`) dengan:

```rust
    if linked > 0 {
        record_and_publish(
            &st,
            &room,
            auth.0,
            "issue.linked",
            serde_json::json!({ "issue_ids": body.issue_ids }),
            &["links"],
        )
        .await?;
    }
```

- [ ] **Step 7: Retrofit `issues_destroy`**

Ganti blok `if result.rows_affected() > 0 { record_event(...) .await?; }` (yang memakai `"issue.unlinked"`) dengan:

```rust
    if result.rows_affected() > 0 {
        record_and_publish(
            &st,
            &room,
            auth.0,
            "issue.unlinked",
            serde_json::json!({ "issue_id": issue_id }),
            &["links"],
        )
        .await?;
    }
```

- [ ] **Step 8: Retrofit `participants_create`**

Ganti `record_event(&st.pool, &room, auth.0, "participant.joined", ...)` (yang setelah `tx.commit()`) dengan:

```rust
    record_and_publish(
        &st,
        &room,
        auth.0,
        "participant.joined",
        serde_json::json!({ "member_id": body.member_id, "role": role }),
        &["participants"],
    )
    .await?;
```

- [ ] **Step 9: Retrofit `participants_patch`**

Ganti `record_event(&st.pool, &room, auth.0, "participant.role_changed", ...)` dengan:

```rust
    record_and_publish(
        &st,
        &room,
        auth.0,
        "participant.role_changed",
        serde_json::json!({
            "member_id": current.member_id, "from": current.role, "to": body.role
        }),
        &["participants"],
    )
    .await?;
```

- [ ] **Step 10: Retrofit `participants_destroy`**

Ganti `record_event(&st.pool, &room, auth.0, "participant.left", ...)` dengan:

```rust
    record_and_publish(
        &st,
        &room,
        auth.0,
        "participant.left",
        serde_json::json!({ "member_id": current.member_id }),
        &["participants"],
    )
    .await?;
```

- [ ] **Step 11: Retrofit `runbook_patch`**

Ganti blok `if is_done != current.is_done { ... record_event(...) ... }` dengan:

```rust
    if is_done != current.is_done {
        let event_type = if is_done {
            "runbook.item_done"
        } else {
            "runbook.item_reopened"
        };
        record_and_publish(
            &st,
            &room,
            auth.0,
            event_type,
            serde_json::json!({ "item_id": item_id, "title": title }),
            &["runbook"],
        )
        .await?;
    }
```

- [ ] **Step 12: Publish di `runbook_create` dan `runbook_destroy`**

Di `runbook_create`, setelah `.execute(&st.pool).await?;` milik INSERT item dan sebelum `let row = runbook_item_by_id(...)`, sisipkan:

```rust
    publish_war_room_event(
        &st,
        room.id,
        "room.changed",
        serde_json::json!({ "reasons": ["runbook"] }),
    )
    .await;
```

Di `runbook_destroy`, setelah `.execute(&st.pool).await?;` milik UPDATE soft delete dan sebelum `Ok((StatusCode::NO_CONTENT, ...))`, sisipkan hal yang sama.

- [ ] **Step 13: Publish di `create` dan `destroy`**

Di `create`, setelah `tx.commit().await?;` dan sebelum `let row = fetch_room(...)`, sisipkan:

```rust
    publish_war_room_event(
        &st,
        room_id,
        "room.changed",
        serde_json::json!({ "reasons": ["created"] }),
    )
    .await;
```

Di `destroy`, setelah `tx.commit().await?;` dan sebelum `Ok((StatusCode::NO_CONTENT, ...))`, sisipkan:

```rust
    publish_war_room_event(
        &st,
        pk,
        "room.changed",
        serde_json::json!({ "reasons": ["deleted"] }),
    )
    .await;
```

- [ ] **Step 14: Verifikasi compile + unit test**

Run: `cargo test -p api --lib war_room::tests`
Expected: PASS (6 tests), tanpa error.

- [ ] **Step 15: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/war_room.rs
git commit -m "feat(api-rs): publish war room realtime events from all write handlers"
```

---

### Task 4: `messages_create` (mentions + notifications + publish)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/war_room.rs`

- [ ] **Step 1: Tambahkan request struct + helper mention + handler**

Tambahkan tepat sebelum `#[cfg(test)] mod tests`:

```rust
#[derive(Debug, Deserialize)]
pub struct MessageCreate {
    pub body: String,
    pub client_id: Option<String>,
}

async fn valid_mention_members(
    pool: &PgPool,
    workspace_id: Uuid,
    candidates: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_scalar(
        "SELECT member_id FROM workspace_members \
         WHERE workspace_id = $1 AND member_id = ANY($2) \
         AND is_active = true AND deleted_at IS NULL",
    )
    .bind(workspace_id)
    .bind(candidates)
    .fetch_all(pool)
    .await
}

/// One `notifications` row per mentioned workspace member (author excluded),
/// shaped like `ai_schedule_run` so the existing card/mentioned filters work.
async fn insert_mention_notifications(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    room: &WarRoomRow,
    actor: Uuid,
    workspace_slug: &str,
    mentions: &[Uuid],
) -> Result<(), sqlx::Error> {
    for receiver in mentions {
        if *receiver == actor {
            continue;
        }
        sqlx::query(
            "INSERT INTO notifications (id, workspace_id, project_id, receiver_id, entity_name, \
             entity_identifier, title, sender, data, message_html, created_at, updated_at, \
             created_by_id, triggered_by_id) \
             VALUES (gen_random_uuid(), $1, $2, $3, 'war_room', $4, $5, \
                     'in_app:war_room:mentioned', $6, '<p></p>', now(), now(), $7, $7)",
        )
        .bind(room.workspace_id)
        .bind(room.project_id)
        .bind(receiver)
        .bind(room.id)
        .bind(&room.name)
        .bind(serde_json::json!({
            "war_room": {
                "id": room.id,
                "project_id": room.project_id,
                "workspace_slug": workspace_slug,
                "name": room.name,
                "sequence_id": room.sequence_id,
            }
        }))
        .bind(actor)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub async fn messages_create(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk)): Path<(String, Uuid, Uuid)>,
    Json(body): Json<MessageCreate>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    let body_text = body.body.trim().to_string();
    if body_text.is_empty() {
        return Ok(bad_request("Invalid body"));
    }
    let mentioned =
        valid_mention_members(&st.pool, room.workspace_id, &parse_mentions(&body_text)).await?;
    let message_id = Uuid::new_v4();
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "INSERT INTO war_room_messages (id, workspace_id, project_id, war_room_id, author_id, \
         body, mentions, created_at, updated_at, created_by_id, updated_by_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now(), $5, $5)",
    )
    .bind(message_id)
    .bind(room.workspace_id)
    .bind(project_id)
    .bind(room.id)
    .bind(auth.0)
    .bind(&body_text)
    .bind(serde_json::json!(mentioned))
    .execute(&mut *tx)
    .await?;
    insert_mention_notifications(&mut tx, &room, auth.0, &slug, &mentioned).await?;
    tx.commit().await?;

    let row = fetch_message(&st.pool, room.id, message_id)
        .await?
        .expect("message just inserted");
    let mut data = message_json(&row);
    data["client_id"] = serde_json::json!(body.client_id);
    publish_war_room_event(&st, room.id, "message.created", data.clone()).await;
    Ok((StatusCode::CREATED, Json(data)))
}
```

- [ ] **Step 2: Verifikasi compile**

Run: `cargo check -p api`
Expected: LULUS.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/war_room.rs
git commit -m "feat(api-rs): war room message create with mentions and notifications"
```

---

### Task 5: `messages_patch` + `messages_destroy` (penulis saja)

**Files:**

- Modify: `apps/api-rs/crates/api/src/routes/war_room.rs`

- [ ] **Step 1: Tambahkan struct + handler**

Tambahkan tepat sebelum `#[cfg(test)] mod tests`:

```rust
#[derive(Debug, Deserialize)]
pub struct MessagePatch {
    pub body: String,
}

pub async fn messages_patch(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk, message_id)): Path<(String, Uuid, Uuid, Uuid)>,
    Json(body): Json<MessagePatch>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    let Some(current) = fetch_message(&st.pool, room.id, message_id).await? else {
        return Ok(missing());
    };
    if current.author_id != Some(auth.0) {
        return Ok(deny());
    }
    let body_text = body.body.trim().to_string();
    if body_text.is_empty() {
        return Ok(bad_request("Invalid body"));
    }
    // Mentions are re-parsed so chips stay correct, but edits never create
    // new notifications (avoids mention-spam on every edit).
    let mentioned =
        valid_mention_members(&st.pool, room.workspace_id, &parse_mentions(&body_text)).await?;
    sqlx::query(
        "UPDATE war_room_messages SET body = $1, mentions = $2, edited_at = now(), \
         updated_at = now(), updated_by_id = $3 \
         WHERE id = $4 AND war_room_id = $5 AND deleted_at IS NULL",
    )
    .bind(&body_text)
    .bind(serde_json::json!(mentioned))
    .bind(auth.0)
    .bind(message_id)
    .bind(room.id)
    .execute(&st.pool)
    .await?;
    let row = fetch_message(&st.pool, room.id, message_id)
        .await?
        .expect("message just updated");
    publish_war_room_event(&st, room.id, "message.updated", message_json(&row)).await;
    Ok((StatusCode::OK, Json(message_json(&row))))
}

pub async fn messages_destroy(
    State(st): State<AppState>,
    auth: AuthUser,
    Path((slug, project_id, pk, message_id)): Path<(String, Uuid, Uuid, Uuid)>,
) -> Result<(StatusCode, Json<Value>), common::errors::AppError> {
    let room = match room_for_write(&st, &slug, project_id, pk, auth.0).await? {
        Ok(room) => room,
        Err(response) => return Ok(response),
    };
    let Some(current) = fetch_message(&st.pool, room.id, message_id).await? else {
        return Ok((StatusCode::NO_CONTENT, Json(Value::Null)));
    };
    if current.author_id != Some(auth.0) {
        return Ok(deny());
    }
    sqlx::query(
        "UPDATE war_room_messages SET deleted_at = now(), updated_at = now() \
         WHERE id = $1 AND war_room_id = $2 AND deleted_at IS NULL",
    )
    .bind(message_id)
    .bind(room.id)
    .execute(&st.pool)
    .await?;
    publish_war_room_event(
        &st,
        room.id,
        "message.deleted",
        serde_json::json!({ "id": message_id, "war_room_id": room.id }),
    )
    .await;
    Ok((StatusCode::NO_CONTENT, Json(Value::Null)))
}
```

- [ ] **Step 2: Verifikasi compile**

Run: `cargo check -p api`
Expected: LULUS tanpa warning dead code untuk message helpers.

- [ ] **Step 3: Commit**

```bash
git add apps/api-rs/crates/api/src/routes/war_room.rs
git commit -m "feat(api-rs): war room message edit and delete, author only"
```

---

### Task 6: Registrasi route messages

**Files:**

- Modify: `apps/api-rs/crates/api/src/main.rs`

- [ ] **Step 1: Tambahkan dua route**

Di `main.rs`, setelah blok route `war-rooms/:pk/events/` yang sudah ada, tambahkan:

```rust
        .route(
            "/api/workspaces/:slug/projects/:project_id/war-rooms/:pk/messages/",
            get(routes::war_room::messages_list).post(routes::war_room::messages_create),
        )
        .route(
            "/api/workspaces/:slug/projects/:project_id/war-rooms/:pk/messages/:message_id/",
            patch(routes::war_room::messages_patch).delete(routes::war_room::messages_destroy),
        )
```

- [ ] **Step 2: Verifikasi route inventory**

Run: `cargo test -p api --test route_inventory_test`
Expected: PASS (5 tests) — tidak ada shape conflict.

- [ ] **Step 3: Build full**

Run: `cargo build -p api`
Expected: LULUS.

- [ ] **Step 4: Commit**

```bash
git add apps/api-rs/crates/api/src/main.rs
git commit -m "feat(api-rs): register war room message routes"
```

---

### Task 7: Test integrasi messages + publish

**Files:**

- Modify: `apps/api-rs/crates/api/tests/war_room_test.rs`
- Modify: `apps/api-rs/crates/api/Cargo.toml`

- [ ] **Step 1: Tambahkan dev-dependency `futures`**

Di `apps/api-rs/crates/api/Cargo.toml`, tambahkan ke `[dev-dependencies]`:

```toml
futures = "0.3"
```

- [ ] **Step 2: Update header + import test file**

Di `war_room_test.rs`, ganti blok `use api::routes::war_room::{...}` menjadi:

```rust
use api::routes::war_room::{
    create, destroy, detail, events_list, issues_create, issues_destroy, list, messages_create,
    messages_destroy, messages_list, messages_patch, participants_create, participants_destroy,
    participants_patch, patch, runbook_create, runbook_patch, services_create, services_destroy,
    summary, CreateWarRoom, EventsParams, LinkIssues, LinkServices, ListParams, MessageCreate,
    MessagePatch, MessagesParams, ParticipantCreate, ParticipantPatch, PatchWarRoom, RunbookCreate,
    RunbookPatch,
};
```

Tambahkan setelah `use api::state::AppState;`:

```rust
use futures::StreamExt;
use std::time::Duration;
```

- [ ] **Step 3: Bersihkan notifications di `cleanup`**

Di `impl Scratch`, `cleanup`, tambahkan sebelum `DELETE FROM workspaces`:

```rust
        // `notifications` FKs are NO ACTION and mention rows reference the
        // scratch workspace/users, so clear them before the workspace delete.
        sqlx::query("DELETE FROM notifications WHERE workspace_id = $1")
            .bind(self.workspace_id)
            .execute(pool)
            .await
            .ok();
```

- [ ] **Step 4: Tambahkan lima test di akhir file**

```rust
#[tokio::test]
async fn messages_cursor_pagination_returns_chronological_pages() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);

    for body in ["first", "second", "third"] {
        let (status, _) = messages_create(
            State(st.clone()),
            AuthUser(scratch.user_id),
            Path((scratch.slug.clone(), scratch.project_id, pk)),
            Json(MessageCreate { body: body.into(), client_id: None }),
        )
        .await
        .expect("create message");
        assert_eq!(status, StatusCode::CREATED);
        // Distinct transaction timestamps keep the cursor ordering deterministic.
        tokio::time::sleep(Duration::from_millis(2)).await;
    }

    let (status, Json(page)) = messages_list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Query(MessagesParams { before_id: None, limit: Some(2) }),
    )
    .await
    .expect("list page");
    assert_eq!(status, StatusCode::OK);
    let bodies: Vec<&str> = page
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["body"].as_str().unwrap())
        .collect();
    assert_eq!(bodies, vec!["second", "third"]);

    let oldest_id = Uuid::parse_str(page[0]["id"].as_str().unwrap()).unwrap();
    let (status, Json(older)) = messages_list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Query(MessagesParams { before_id: Some(oldest_id), limit: Some(2) }),
    )
    .await
    .expect("list older");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(older.as_array().unwrap().len(), 1);
    assert_eq!(older[0]["body"], "first");

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn message_mentions_notify_workspace_members_only() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let teammate = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    let outsider = scratch.add_actor(&st.pool, None, None).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);

    let body = format!(
        "halo @{{{teammate}}} @{{{outsider}}} @{{{}}}",
        scratch.user_id
    );
    let (status, Json(message)) = messages_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(MessageCreate { body, client_id: Some("client-1".into()) }),
    )
    .await
    .expect("create mention");
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(message["client_id"], "client-1");
    let mentions: Vec<&str> = message["mentions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap())
        .collect();
    assert_eq!(mentions.len(), 2);
    assert!(mentions.contains(&teammate.to_string().as_str()));
    assert!(mentions.contains(&scratch.user_id.to_string().as_str()));

    let notified: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE entity_name = 'war_room' \
         AND entity_identifier = $1 AND receiver_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(teammate)
    .fetch_one(&st.pool)
    .await
    .expect("count notifications");
    assert_eq!(notified, 1);

    let author_notified: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE entity_name = 'war_room' \
         AND entity_identifier = $1 AND receiver_id = $2 AND deleted_at IS NULL",
    )
    .bind(pk)
    .bind(scratch.user_id)
    .fetch_one(&st.pool)
    .await
    .expect("count author notifications");
    assert_eq!(author_notified, 0);

    let (status, Json(body)) = messages_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(MessageCreate { body: "   ".into(), client_id: None }),
    )
    .await
    .expect("empty body");
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].is_string());

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn message_edit_and_delete_are_author_only() {
    let st = state().await;
    let mut scratch = Scratch::new(&st.pool).await;
    let teammate = scratch.add_actor(&st.pool, Some(15), Some(15)).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);

    let (_, Json(message)) = messages_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(MessageCreate { body: "original".into(), client_id: None }),
    )
    .await
    .expect("create");
    let message_id = Uuid::parse_str(message["id"].as_str().unwrap()).unwrap();

    let (status, _) = messages_patch(
        State(st.clone()),
        AuthUser(teammate),
        Path((scratch.slug.clone(), scratch.project_id, pk, message_id)),
        Json(MessagePatch { body: "hijack".into() }),
    )
    .await
    .expect("teammate edit");
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = messages_destroy(
        State(st.clone()),
        AuthUser(teammate),
        Path((scratch.slug.clone(), scratch.project_id, pk, message_id)),
    )
    .await
    .expect("teammate delete");
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, Json(edited)) = messages_patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk, message_id)),
        Json(MessagePatch { body: "edited".into() }),
    )
    .await
    .expect("author edit");
    assert_eq!(status, StatusCode::OK);
    assert_eq!(edited["body"], "edited");
    assert!(edited["edited_at"].is_string());

    let (status, _) = messages_destroy(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk, message_id)),
    )
    .await
    .expect("author delete");
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, Json(page)) = messages_list(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Query(MessagesParams { before_id: None, limit: None }),
    )
    .await
    .expect("list after delete");
    assert!(page.as_array().unwrap().is_empty());

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn archived_room_rejects_message_writes() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;
    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);

    let (status, _) = patch(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(PatchWarRoom {
            name: None,
            severity: None,
            status: Some("archived".into()),
            description_html: None,
            notes_html: None,
        }),
    )
    .await
    .expect("archive");
    assert_eq!(status, StatusCode::OK);

    let (status, Json(body)) = messages_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(MessageCreate { body: "too late".into(), client_id: None }),
    )
    .await
    .expect("archived message");
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "room_archived");

    scratch.cleanup(&st.pool).await;
}

#[tokio::test]
async fn message_create_publishes_realtime_event() {
    let st = state().await;
    let scratch = Scratch::new(&st.pool).await;

    let client = redis::Client::open("redis://127.0.0.1:6379").expect("redis client");
    let Ok(conn) = client.get_async_connection().await else {
        eprintln!("redis unavailable, skipping publish assertion");
        scratch.cleanup(&st.pool).await;
        return;
    };
    let mut pubsub = conn.into_pubsub();
    pubsub
        .subscribe("war-room:events")
        .await
        .expect("subscribe");
    let mut stream = pubsub.on_message();

    let issue_id = scratch.insert_issue(&st.pool, None).await;
    let room = create_room(&st, &scratch, issue_id).await;
    let pk = room_id(&room);

    let (_, Json(message)) = messages_create(
        State(st.clone()),
        AuthUser(scratch.user_id),
        Path((scratch.slug.clone(), scratch.project_id, pk)),
        Json(MessageCreate { body: "ping room".into(), client_id: Some("client-9".into()) }),
    )
    .await
    .expect("create message");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let Ok(Some(msg)) = tokio::time::timeout(remaining, stream.next()).await else {
            panic!("no message.created event published before deadline");
        };
        let payload: String = msg.get_payload().expect("payload");
        let value: Value = serde_json::from_str(&payload).expect("json payload");
        if value["kind"] == "message.created" {
            assert_eq!(value["room_id"], pk.to_string());
            assert_eq!(value["data"]["id"], message["id"]);
            assert_eq!(value["data"]["body"], "ping room");
            assert_eq!(value["data"]["client_id"], "client-9");
            break;
        }
    }

    scratch.cleanup(&st.pool).await;
}
```

- [ ] **Step 5: Jalankan suite war room**

Run:

```bash
DATABASE_URL=postgres://plane:plane@localhost:5432/plane \
  cargo test -p api --test war_room_test -- --test-threads=1
```

Expected: PASS (13 tests). Perbaiki hanya scratch/assertion bila kolom NOT NULL kurang; jangan longgarkan assertion handler.

- [ ] **Step 6: Commit**

```bash
git add apps/api-rs/crates/api/Cargo.toml apps/api-rs/crates/api/tests/war_room_test.rs
git commit -m "test(api-rs): war room message handlers and realtime publish integration tests"
```

---

### Task 8: Live — tipe, parser handshake, service cek akses

**Files:**

- Create: `apps/live/src/types/war-room.ts`
- Modify: `apps/live/src/types/index.ts`
- Create: `apps/live/src/lib/war-room-auth.ts`
- Create: `apps/live/src/services/war-room.service.ts`
- Test: `apps/live/tests/lib/war-room-auth.test.ts`

- [ ] **Step 1: Tulis test parser yang gagal**

Create `apps/live/tests/lib/war-room-auth.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import { describe, it, expect } from "vitest";
import { parseWarRoomHandshake } from "@/lib/war-room-auth";

describe("parseWarRoomHandshake", () => {
  const roomId = "11111111-1111-1111-1111-111111111111";

  it("extracts userId from token and cookie from headers", () => {
    const result = parseWarRoomHandshake({
      headers: { cookie: "session=abc" },
      params: { roomId },
      query: { workspaceSlug: "acme", projectId: "p-1", token: JSON.stringify({ id: "u-1" }) },
    });
    expect(result).toEqual({
      userId: "u-1",
      cookie: "session=abc",
      roomId,
      workspaceSlug: "acme",
      projectId: "p-1",
    });
  });

  it("falls back to a cookie embedded in the token", () => {
    const result = parseWarRoomHandshake({
      headers: {},
      params: { roomId },
      query: {
        workspaceSlug: "acme",
        projectId: "p-1",
        token: JSON.stringify({ id: "u-1", cookie: "session=token-cookie" }),
      },
    });
    expect(result?.cookie).toBe("session=token-cookie");
  });

  it("rejects missing query params and malformed tokens", () => {
    expect(
      parseWarRoomHandshake({
        headers: { cookie: "session=abc" },
        params: { roomId },
        query: { projectId: "p-1", token: JSON.stringify({ id: "u-1" }) },
      })
    ).toBeNull();
    expect(
      parseWarRoomHandshake({
        headers: { cookie: "session=abc" },
        params: { roomId },
        query: { workspaceSlug: "acme", projectId: "p-1", token: "not-json" },
      })
    ).toBeNull();
  });

  it("rejects when no cookie is available at all", () => {
    expect(
      parseWarRoomHandshake({
        headers: {},
        params: { roomId },
        query: { workspaceSlug: "acme", projectId: "p-1", token: JSON.stringify({ id: "u-1" }) },
      })
    ).toBeNull();
  });
});
```

- [ ] **Step 2: Run test untuk memastikan gagal**

Run: `pnpm --filter=live test -- tests/lib/war-room-auth.test.ts`
Expected: FAIL — modul `@/lib/war-room-auth` belum ada.

- [ ] **Step 3: Tulis tipe + parser + service**

Create `apps/live/src/types/war-room.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

export type TWarRoomRelayMeta = {
  userId: string;
  name: string;
};

export type TWarRoomRedisPayload = {
  room_id: string;
  kind: string;
  data: unknown;
};

export type TWarRoomClientMessage = { type: "typing"; is_typing?: boolean } | { type: "ping" };

export type TWarRoomHandshake = {
  userId: string;
  cookie: string;
  roomId: string;
  workspaceSlug: string;
  projectId: string;
};
```

Tambahkan di akhir `apps/live/src/types/index.ts`:

```ts
export * from "./war-room";
```

Create `apps/live/src/lib/war-room-auth.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { IncomingHttpHeaders } from "http";
// types
import type { TWarRoomHandshake } from "@/types";

/**
 * Parse the war room WS handshake from the express request. Mirrors the
 * collaboration auth: the token is `JSON.stringify(user)`, the session cookie
 * comes from the handshake headers (same host, different port → cookie is
 * sent), with an optional cookie embedded in the token as fallback.
 */
export const parseWarRoomHandshake = ({
  headers,
  params,
  query,
}: {
  headers: IncomingHttpHeaders;
  params: Record<string, string | undefined>;
  query: Record<string, unknown>;
}): TWarRoomHandshake | null => {
  const roomId = params.roomId;
  const workspaceSlug = typeof query.workspaceSlug === "string" ? query.workspaceSlug : undefined;
  const projectId = typeof query.projectId === "string" ? query.projectId : undefined;
  const rawToken = typeof query.token === "string" ? query.token : undefined;
  if (!roomId || !workspaceSlug || !projectId || !rawToken) return null;

  let userId: string | undefined;
  let tokenCookie: string | undefined;
  try {
    const parsed = JSON.parse(rawToken) as { id?: string; cookie?: string };
    userId = parsed.id;
    tokenCookie = parsed.cookie;
  } catch {
    return null;
  }
  const cookie = tokenCookie || headers.cookie?.toString();
  if (!userId || !cookie) return null;

  return { userId, cookie, roomId, workspaceSlug, projectId };
};
```

Create `apps/live/src/services/war-room.service.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

// plane imports
import { logger } from "@plane/logger";
// services
import { APIService } from "@/services/api.service";

export class WarRoomService extends APIService {
  constructor() {
    super();
  }

  /**
   * Reuse the REST detail endpoint as the membership gate: 200 = the cookie's
   * user can read the room (guest included), 403/404 = reject the socket.
   */
  async validateAccess({
    workspaceSlug,
    projectId,
    roomId,
    cookie,
  }: {
    workspaceSlug: string;
    projectId: string;
    roomId: string;
    cookie: string;
  }): Promise<boolean> {
    try {
      await this.get(`/api/workspaces/${workspaceSlug}/projects/${projectId}/war-rooms/${roomId}/`, {
        headers: { Cookie: cookie },
      });
      return true;
    } catch (error) {
      logger.warn("WAR_ROOM_SERVICE: access check failed", error);
      return false;
    }
  }
}
```

- [ ] **Step 4: Run test untuk memastikan lulus**

Run: `pnpm --filter=live test -- tests/lib/war-room-auth.test.ts`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/live/src/types/war-room.ts apps/live/src/types/index.ts \
  apps/live/src/lib/war-room-auth.ts apps/live/src/services/war-room.service.ts \
  apps/live/tests/lib/war-room-auth.test.ts
git commit -m "feat(live): war room handshake parser and access check service"
```

---

### Task 9: Live — relay (registry + Redis subscribe + fan-out)

**Files:**

- Create: `apps/live/src/services/war-room-relay.service.ts`
- Test: `apps/live/tests/services/war-room-relay.test.ts`

- [ ] **Step 1: Tulis test relay yang gagal**

Create `apps/live/tests/services/war-room-relay.test.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

process.env.API_BASE_URL = "http://localhost:8000";
process.env.LIVE_SERVER_SECRET_KEY = "test-secret";

const { WarRoomRelay } = await import("@/services/war-room-relay.service");

type FakeSocket = {
  readyState: number;
  sent: string[];
  send: (message: string) => void;
};

const fakeSocket = (): FakeSocket => ({
  readyState: 1,
  sent: [],
  send(message: string) {
    this.sent.push(message);
  },
});

describe("WarRoomRelay", () => {
  it("fans out Redis payloads to sockets of the matching room only", () => {
    const relay = new WarRoomRelay();
    const a = fakeSocket();
    const b = fakeSocket();
    relay.join("room-1", a as never, { userId: "u-1", name: "One" }, false);
    relay.join("room-2", b as never, { userId: "u-2", name: "Two" }, false);

    relay.handleRedisPayload(JSON.stringify({ room_id: "room-1", kind: "message.created", data: { body: "hi" } }));

    expect(a.sent).toHaveLength(1);
    expect(JSON.parse(a.sent[0])).toEqual({ kind: "message.created", data: { body: "hi" } });
    expect(b.sent).toHaveLength(0);
  });

  it("ignores malformed payloads", () => {
    const relay = new WarRoomRelay();
    const a = fakeSocket();
    relay.join("room-1", a as never, { userId: "u-1", name: "One" }, false);
    expect(() => relay.handleRedisPayload("not-json")).not.toThrow();
    expect(a.sent).toHaveLength(0);
  });

  it("join and leave are idempotent and drop empty rooms", () => {
    const relay = new WarRoomRelay();
    const a = fakeSocket();
    relay.join("room-1", a as never, { userId: "u-1", name: "One" }, false);
    relay.join("room-1", a as never, { userId: "u-1", name: "One" }, false);
    expect(relay.roomSize("room-1")).toBe(1);
    relay.leave("room-1", a as never);
    relay.leave("room-1", a as never);
    expect(relay.roomSize("room-1")).toBe(0);
  });

  it("skips sockets that are not open", () => {
    const relay = new WarRoomRelay();
    const closed = fakeSocket();
    closed.readyState = 3;
    relay.join("room-1", closed as never, { userId: "u-1", name: "One" }, false);
    relay.handleRedisPayload(JSON.stringify({ room_id: "room-1", kind: "typing", data: {} }));
    expect(closed.sent).toHaveLength(0);
  });
});
```

- [ ] **Step 2: Run test untuk memastikan gagal**

Run: `pnpm --filter=live test -- tests/services/war-room-relay.test.ts`
Expected: FAIL — modul relay belum ada.

- [ ] **Step 3: Implementasi relay**

Create `apps/live/src/services/war-room-relay.service.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type Redis from "ioredis";
import type WebSocket from "ws";
// plane imports
import { logger } from "@plane/logger";
// helpers
import { redisManager } from "@/redis";
// types
import type { TWarRoomRedisPayload, TWarRoomRelayMeta } from "@/types";

export const WAR_ROOM_CHANNEL = "war-room:events";

type RelaySocket = {
  ws: WebSocket;
  meta: TWarRoomRelayMeta;
};

/**
 * In-memory room registry + Redis bridge. api-rs publishes `{room_id, kind,
 * data}`; every live instance relays to its local sockets. Typing/presence
 * publish through the same channel so multi-instance fan-out stays uniform.
 */
export class WarRoomRelay {
  private rooms = new Map<string, RelaySocket[]>();
  private subscriber: Redis | null = null;

  async initialize(): Promise<void> {
    if (this.subscriber) return;
    const base = redisManager.getClient();
    if (!base) {
      logger.warn("WAR_ROOM_RELAY: Redis unavailable, relay disabled");
      return;
    }
    const subscriber = base.duplicate();
    await new Promise<void>((resolve, reject) => {
      subscriber.subscribe(WAR_ROOM_CHANNEL, (error) => {
        if (error) reject(error);
        else resolve();
      });
    });
    subscriber.on("message", (channel: string, message: string) => {
      if (channel !== WAR_ROOM_CHANNEL) return;
      this.handleRedisPayload(message);
    });
    this.subscriber = subscriber;
    logger.info(`WAR_ROOM_RELAY: subscribed to ${WAR_ROOM_CHANNEL}`);
  }

  async destroy(): Promise<void> {
    if (!this.subscriber) return;
    const subscriber = this.subscriber;
    this.subscriber = null;
    try {
      await subscriber.quit();
    } catch (error) {
      logger.error("WAR_ROOM_RELAY: error quitting subscriber", error);
      subscriber.disconnect();
    }
    this.rooms.clear();
  }

  join(roomId: string, ws: WebSocket, meta: TWarRoomRelayMeta, announce = true): void {
    const sockets = this.rooms.get(roomId) ?? [];
    sockets.push({ ws, meta });
    this.rooms.set(roomId, sockets);
    if (announce) {
      void this.publish("presence.joined", roomId, { user_id: meta.userId, name: meta.name });
    }
  }

  leave(roomId: string, ws: WebSocket, announce = true): void {
    const sockets = this.rooms.get(roomId);
    if (!sockets) return;
    const index = sockets.findIndex((entry) => entry.ws === ws);
    if (index === -1) return;
    const [entry] = sockets.splice(index, 1);
    if (sockets.length === 0) this.rooms.delete(roomId);
    if (announce) {
      void this.publish("presence.left", roomId, { user_id: entry.meta.userId, name: entry.meta.name });
    }
  }

  roomSize(roomId: string): number {
    return this.rooms.get(roomId)?.length ?? 0;
  }

  handleRedisPayload(message: string): void {
    try {
      const payload = JSON.parse(message) as TWarRoomRedisPayload;
      if (!payload?.room_id || !payload.kind) return;
      this.fanOut(payload.room_id, { kind: payload.kind, data: payload.data });
    } catch (error) {
      logger.error("WAR_ROOM_RELAY: malformed Redis payload", error);
    }
  }

  fanOut(roomId: string, event: { kind: string; data: unknown }): void {
    const sockets = this.rooms.get(roomId);
    if (!sockets || sockets.length === 0) return;
    const message = JSON.stringify(event);
    for (const { ws } of sockets) {
      if (ws.readyState !== 1) continue;
      try {
        ws.send(message);
      } catch (error) {
        logger.error("WAR_ROOM_RELAY: send failed", error);
      }
    }
  }

  async publish(kind: string, roomId: string, data: unknown): Promise<void> {
    const client = redisManager.getClient();
    if (!client) return;
    try {
      await client.publish(WAR_ROOM_CHANNEL, JSON.stringify({ room_id: roomId, kind, data }));
    } catch (error) {
      logger.error("WAR_ROOM_RELAY: publish failed", error);
    }
  }
}

export const warRoomRelay = new WarRoomRelay();
```

- [ ] **Step 4: Run test untuk memastikan lulus**

Run: `pnpm --filter=live test -- tests/services/war-room-relay.test.ts`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/live/src/services/war-room-relay.service.ts apps/live/tests/services/war-room-relay.test.ts
git commit -m "feat(live): war room relay registry with Redis fan-out"
```

---

### Task 10: Live — controller + wiring server

**Files:**

- Create: `apps/live/src/controllers/war-room.controller.ts`
- Modify: `apps/live/src/controllers/index.ts`
- Modify: `apps/live/src/server.ts`

- [ ] **Step 1: Tulis controller**

Create `apps/live/src/controllers/war-room.controller.ts`:

```ts
/**
 * Copyright (c) 2023-present Plane Software, Inc. and contributors
 * SPDX-License-Identifier: AGPL-3.0-only
 * See the LICENSE file for details.
 */

import type { Request } from "express";
import type WebSocket from "ws";
import type { RawData } from "ws";
// plane imports
import { Controller, WebSocket as WSDecorator } from "@plane/decorators";
import { logger } from "@plane/logger";
// helpers
import { handleAuthentication } from "@/lib/auth";
import { parseWarRoomHandshake } from "@/lib/war-room-auth";
// services
import { warRoomRelay } from "@/services/war-room-relay.service";
import { WarRoomService } from "@/services/war-room.service";
// types
import type { TWarRoomClientMessage } from "@/types";

@Controller("/war-rooms")
export class WarRoomController {
  [key: string]: unknown;

  @WSDecorator("/:roomId")
  handleConnection(ws: WebSocket, req: Request) {
    void this.upgrade(ws, req).catch((error) => {
      logger.error("WAR_ROOM_CONTROLLER: connection rejected", error);
      try {
        ws.close(4403, "Unauthorized");
      } catch {
        // socket already closed
      }
    });
  }

  private async upgrade(ws: WebSocket, req: Request) {
    const handshake = parseWarRoomHandshake({
      headers: req.headers,
      params: req.params,
      query: req.query,
    });
    if (!handshake) {
      ws.close(4403, "Unauthorized");
      return;
    }
    const auth = await handleAuthentication({
      cookie: handshake.cookie,
      userId: handshake.userId,
    });
    const warRoomService = new WarRoomService();
    const allowed = await warRoomService.validateAccess({
      workspaceSlug: handshake.workspaceSlug,
      projectId: handshake.projectId,
      roomId: handshake.roomId,
      cookie: handshake.cookie,
    });
    if (!allowed) {
      ws.close(4403, "Forbidden");
      return;
    }

    warRoomRelay.join(handshake.roomId, ws, { userId: auth.user.id, name: auth.user.name });
    ws.on("message", (raw: RawData) => this.handleClientMessage(handshake.roomId, auth.user.id, ws, raw));
    ws.on("close", () => warRoomRelay.leave(handshake.roomId, ws));
    ws.on("error", (error: Error) => {
      logger.error("WAR_ROOM_CONTROLLER: socket error", error);
      warRoomRelay.leave(handshake.roomId, ws);
    });
  }

  private handleClientMessage(roomId: string, userId: string, ws: WebSocket, raw: RawData) {
    let parsed: TWarRoomClientMessage;
    try {
      parsed = JSON.parse(raw.toString()) as TWarRoomClientMessage;
    } catch {
      return;
    }
    if (parsed.type === "typing") {
      void warRoomRelay.publish("typing", roomId, {
        user_id: userId,
        is_typing: parsed.is_typing !== false,
      });
    }
    // App-level heartbeat (client sends every 30s); answer directly, no Redis.
    if (parsed.type === "ping" && ws.readyState === 1) {
      ws.send(JSON.stringify({ kind: "pong" }));
    }
  }
}
```

- [ ] **Step 2: Daftarkan controller**

Di `apps/live/src/controllers/index.ts`, tambahkan import dan entri list:

```ts
import { WarRoomController } from "./war-room.controller";
```

```ts
export const CONTROLLERS = [
  CollaborationController,
  DocumentController,
  HealthController,
  PdfExportController,
  WarRoomController,
];
```

- [ ] **Step 3: Wiring relay di server lifecycle**

Di `apps/live/src/server.ts`:

Tambahkan import:

```ts
import { warRoomRelay } from "@/services/war-room-relay.service";
```

Di `initialize()`, setelah `await redisManager.initialize();` tambahkan:

```ts
await warRoomRelay.initialize();
logger.info("SERVER: War room relay setup completed");
```

Di `destroy()`, sebelum `await redisManager.disconnect();` tambahkan:

```ts
await warRoomRelay.destroy();
logger.info("SERVER: War room relay closed gracefully.");
```

- [ ] **Step 4: Typecheck + test + build**

Run:

```bash
pnpm --filter=live check:types
pnpm --filter=live test
pnpm --filter=live build
```

Expected: typecheck LULUS, semua test PASS (termasuk 8 test baru), build LULUS (`dist/` ter-update).

- [ ] **Step 5: Commit**

```bash
git add apps/live/src/controllers/war-room.controller.ts apps/live/src/controllers/index.ts \
  apps/live/src/server.ts
git commit -m "feat(live): war room websocket controller with typing and presence"
```

---

### Task 11: Verifikasi penuh + rollout

**Files:**

- Tidak ada file baru.

- [ ] **Step 1: Suite Rust penuh**

Run:

```bash
cd apps/api-rs
cargo test -p api --lib war_room::tests
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test war_room_test -- --test-threads=1
DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api --test route_inventory_test
```

Expected: 6 / 13 / 5 PASS.

Lalu suite penuh (serial, hindari flake env antar-binary):

```bash
setsid bash -c 'DATABASE_URL=postgres://plane:plane@localhost:5432/plane cargo test -p api -- --test-threads=1 > /tmp/plane-warroom-phase2-fulltest.log 2>&1' < /dev/null > /dev/null 2>&1 &
```

Poll `grep -cE "^test result: ok" /tmp/plane-warroom-phase2-fulltest.log` sampai proses selesai; expected: 0 `FAILED`, total passed ≥ 1284 (1277 fase 1 + 2 unit parser mention + 5 test integrasi messages/publish).

- [ ] **Step 2: Live checks ulang**

Run:

```bash
pnpm --filter=live check:lint
pnpm --filter=live check:types
pnpm --filter=live test
```

Expected: LULUS semua.

- [ ] **Step 3: Rebuild api-rs + restart live (rollout)**

Ikuti `AGENTS.md` (LTO link bisa 10+ menit tanpa output):

```bash
cd /home/ghifari/plane-for-itsm
setsid docker compose -f docker-compose-local.yml up -d --build api worker beat-worker > /tmp/plane-api-build-phase2.log 2>&1 < /dev/null &
```

Poll log sampai selesai, lalu:

```bash
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:8000/health
systemctl --user restart plane-live.service
sleep 10
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:3100/live/health/
```

Expected: `200` dan `200`.

- [ ] **Step 4: Smoke WS — route terdaftar + auth menolak tanpa sesi**

Run:

```bash
node -e "
const WebSocket = require('/home/ghifari/plane-for-itsm/apps/live/node_modules/ws');
const room = '00000000-0000-0000-0000-000000000001';
const url = 'ws://localhost:3100/live/war-rooms/' + room +
  '?workspaceSlug=demo&projectId=00000000-0000-0000-0000-000000000002' +
  '&token=' + encodeURIComponent(JSON.stringify({ id: '00000000-0000-0000-0000-000000000003' }));
const ws = new WebSocket(url);
ws.on('close', (code) => { console.log('closed', code); process.exit(code === 4403 ? 0 : 1); });
ws.on('error', (error) => { console.error('ws error', error.message); process.exit(1); });
setTimeout(() => { console.error('timeout waiting for close'); process.exit(1); }, 8000);
"
```

Expected: `closed 4403` — route hidup, handshake tanpa cookie ditolak.

Catatan: smoke end-to-end dua browser (chat/presence) dilakukan di Fase 4 setelah room page ada; pada fase ini jaminan realtime dipegang unit test relay + test publish Redis Rust.

- [ ] **Step 5: Commit sisa bila lint/format menyentuh file fase 2**

Run:

```bash
git status --short
git add apps/api-rs/crates/api/src/routes/war_room.rs \
  apps/api-rs/crates/api/tests/war_room_test.rs \
  apps/live/src apps/live/tests
git commit -m "chore: phase 2 lint and format fixes"
```

Hanya lakukan bila `git status` menunjukkan file fase 2 yang belum ter-commit; jangan sertakan perubahan lama yang tidak terkait.

---

## Catatan untuk fase berikutnya (bukan bagian plan ini)

- **Fase 3 (web list + create):** types/constants/store/service, halaman list, modal create, single-select picker, sidebar, entry point work item.
- **Fase 4 (room page):** header/lifecycle, graph refactor + peta blast radius, chat panel + hook `use-war-room-socket` (konsumsi `messages` + WS di plan ini), tab Notes/Work items/Runbook/Activity/People. Di sinilah smoke dua browser dilakukan.
- **Fase 5 (notifikasi + docs):** cabang notification card (`entity_name = war_room`, `data.war_room`), `docs/features/war-rooms.md`, update backlog.
- Kontrak yang sudah dibekukan fase ini: channel `war-room:events`, payload `{room_id, kind, data}`, kinds `message.created|updated|deleted`, `room.changed{reasons}`, `activity.created`, `typing`, `presence.joined|left`, dan cursor `messages?before_id=&limit=`.
