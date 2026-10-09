-- Cleanse release packages + RCB/TCB (review control) — fitur dihapus 2026-10-10.
-- Tabel kosong saat drop; index/RLS/constraint ikut terhapus via CASCADE.
DROP TABLE IF EXISTS public.review_session_items CASCADE;
DROP TABLE IF EXISTS public.review_session_participants CASCADE;
DROP TABLE IF EXISTS public.review_sessions CASCADE;
DROP TABLE IF EXISTS public.review_requests CASCADE;
DROP TABLE IF EXISTS public.release_changes CASCADE;
DROP TABLE IF EXISTS public.releases CASCADE;

-- Notifikasi in-app entity review (2 baris di live DB per 2026-10-10).
DELETE FROM public.notifications WHERE entity_name IN ('review_request', 'review_session');
