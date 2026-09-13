# Batch I — project-domain inventory expansion

Date: 2026-09-09 | Branch: `preview` | Label: `Batch I Tn`

## Source

`apps/api/plane/app/urls/project.py` — 20 `path()` entries (lines 25–131). One already inventoried (Batch F T6: `project-views/`, line 92–96). **19 new entries** → `domains.project.endpoints` grows 1 → 20.

Canonicalization: `<str:slug>` → `:slug`, `<uuid:pk>` → `:pk`, `<uuid:project_id>` → `:project_id`, `<uuid:member_id>` → `:member_id`.

## Tasks

### I1 — project CRUD core (4 entries) → `IN PROGRESS`

| path                                         | methods (Django)             | django_source                                | rust_handler hint                                                     |
| -------------------------------------------- | ---------------------------- | -------------------------------------------- | --------------------------------------------------------------------- |
| `/api/workspaces/:slug/projects/`            | GET, POST (`list`, `create`) | `project.py:25` (ProjectViewSet)             | `routes::project::list` / `create`                                    |
| `/api/workspaces/:slug/projects/details/`    | GET (`list_detail`)          | `project.py:30` (ProjectViewSet.list_detail) | `routes::project::project_details`                                    |
| `/api/workspaces/:slug/projects/:pk/`        | GET, PUT, PATCH, DELETE      | `project.py:35` (ProjectViewSet)             | `routes::project::detail` / `patch` / `destroy` — PUT present? verify |
| `/api/workspaces/:slug/project-identifiers/` | verify Endpoint methods      | `project.py:47` (ProjectIdentifierEndpoint)  | `routes::project::check_identifier`                                   |

- [ ] Append 4 entries (`batch_task: "Batch I T1"`, same 10 keys, `fe_pages: []` default)
- [ ] `python3 -m json.tool` exit 0
- [ ] `cargo test -p api --test route_inventory_test && cargo test -p api --test fe_tripwire_test` (3+3 PASS)
- [ ] Presence check 4/4 → commit `feat(rs-api): inventory project CRUD core (Batch I T1)`

### I2 — project invitations + join (5 entries)

| path                                                          | methods (Django)        | django_source                                   | rust_handler hint                                  |
| ------------------------------------------------------------- | ----------------------- | ----------------------------------------------- | -------------------------------------------------- |
| `/api/workspaces/:slug/projects/:project_id/invitations/`     | GET, POST               | `project.py:52` (ProjectInvitationsViewset)     | `routes::invite::proj_list` / `proj_create`        |
| `/api/workspaces/:slug/projects/:project_id/invitations/:pk/` | GET, DELETE             | `project.py:57`                                 | `routes::invite::proj_detail` / `proj_destroy`     |
| `/api/users/me/workspaces/:slug/projects/invitations/`        | GET, POST               | `project.py:62` (UserProjectInvitationsViewset) | verify (me-scoped invite handlers?)                |
| `/api/users/me/workspaces/:slug/project-roles/`               | verify Endpoint methods | `project.py:67` (UserProjectRolesEndpoint)      | verify                                             |
| `/api/workspaces/:slug/projects/:project_id/join/:pk/`        | verify Endpoint methods | `project.py:72` (ProjectJoinEndpoint)           | `routes::invite::proj_join_get` / `proj_join_post` |

- [ ] Same 5 steps → commit `feat(rs-api): inventory project invitations (Batch I T2)`

### I3 — project members (4 entries)

| path                                                             | methods (Django)        | django_source                               | rust_handler hint                              |
| ---------------------------------------------------------------- | ----------------------- | ------------------------------------------- | ---------------------------------------------- |
| `/api/workspaces/:slug/projects/:project_id/members/`            | GET, POST               | `project.py:77` (ProjectMemberViewSet)      | `routes::member::list` / `create`              |
| `/api/workspaces/:slug/projects/:project_id/members/:pk/`        | GET, PATCH, DELETE      | `project.py:82`                             | `routes::member::detail` / `patch` / `destroy` |
| `/api/workspaces/:slug/projects/:project_id/members/leave/`      | POST (`leave` action)   | `project.py:87`                             | `routes::member::leave_project`                |
| `/api/workspaces/:slug/projects/:project_id/project-members/me/` | verify Endpoint methods | `project.py:97` (ProjectMemberUserEndpoint) | `routes::project::my_membership`               |

- [ ] Same 5 steps → commit `feat(rs-api): inventory project members (Batch I T3)`

### I4 — favorites + deploy boards + archive + prefs (6 entries)

| path                                                                        | methods (Django)        | django_source                                      | rust_handler hint                         |
| --------------------------------------------------------------------------- | ----------------------- | -------------------------------------------------- | ----------------------------------------- |
| `/api/workspaces/:slug/user-favorite-projects/`                             | GET, POST               | `project.py:102` (ProjectFavoritesViewSet)         | `routes::project::fav_add` (+list?)       |
| `/api/workspaces/:slug/user-favorite-projects/:project_id/`                 | DELETE                  | `project.py:107`                                   | `routes::project::fav_remove`             |
| `/api/workspaces/:slug/projects/:project_id/project-deploy-boards/`         | GET, POST               | `project.py:112` (DeployBoardViewSet)              | verify (deploy-board handlers?)           |
| `/api/workspaces/:slug/projects/:project_id/project-deploy-boards/:pk/`     | GET, PATCH, DELETE      | `project.py:117`                                   | verify                                    |
| `/api/workspaces/:slug/projects/:project_id/archive/`                       | verify Endpoint methods | `project.py:122` (ProjectArchiveUnarchiveEndpoint) | `routes::project::archive` / `unarchive`  |
| `/api/workspaces/:slug/projects/:project_id/preferences/member/:member_id/` | verify Endpoint methods | `project.py:127` (ProjectMemberPreferenceEndpoint) | `routes::member::pref_get` / `pref_patch` |

- [ ] Same 5 steps → commit `feat(rs-api): inventory project favorites/boards/archive (Batch I T4)`

### I5 — final audit + full suite

- [ ] Audit: all 20 project.py paths inventoried; `domains.project` has exactly 20 entries; `cargo test -p api` 0 failed. No commit (verification only).

## Standing rules (from Batch G/H)

- One task = one subagent → controller verifies (presence check + gates) → quality-review subagent → next task.
- Status recipe: (a) status codes, (b) response keys, (c) error strings, (d) permission gates.
- `methods` = Django-served; never scope down. `fe_pages: []` default.
- Commit format: `feat(rs-api): inventory <scope> (Batch I Tn)`; single-file commits.
