\set ON_ERROR_STOP on

-- Move an existing demo dataset without recreating its objects or data.
BEGIN;
DO $$
DECLARE
    obj record;
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = 'demo') THEN
        RETURN;
    END IF;

    IF EXISTS (
        SELECT 1 FROM pg_class d
        JOIN pg_namespace dn ON dn.oid = d.relnamespace AND dn.nspname = 'demo'
        JOIN pg_namespace pn ON pn.nspname = 'public'
        JOIN pg_class p ON p.relnamespace = pn.oid AND p.relname = d.relname
    ) OR EXISTS (
        SELECT 1 FROM pg_type d
        JOIN pg_namespace dn ON dn.oid = d.typnamespace AND dn.nspname = 'demo'
        JOIN pg_namespace pn ON pn.nspname = 'public'
        JOIN pg_type p ON p.typnamespace = pn.oid AND p.typname = d.typname
    ) OR EXISTS (
        SELECT 1 FROM pg_proc d
        JOIN pg_namespace dn ON dn.oid = d.pronamespace AND dn.nspname = 'demo'
        JOIN pg_namespace pn ON pn.nspname = 'public'
        JOIN pg_proc p ON p.pronamespace = pn.oid AND p.proname = d.proname
            AND p.proargtypes = d.proargtypes
    ) THEN
        RAISE EXCEPTION 'Cannot migrate demo: an object already exists in public';
    END IF;

    FOR obj IN SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'demo' AND c.relkind IN ('r', 'p') ORDER BY c.relname
    LOOP
        EXECUTE format('ALTER TABLE demo.%I SET SCHEMA public', obj.relname);
    END LOOP;
    FOR obj IN SELECT c.relname, c.relkind FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'demo' AND c.relkind IN ('S', 'v', 'm') ORDER BY c.relkind, c.relname
    LOOP
        EXECUTE format('ALTER %s demo.%I SET SCHEMA public',
            CASE obj.relkind WHEN 'S' THEN 'SEQUENCE' WHEN 'v' THEN 'VIEW' ELSE 'MATERIALIZED VIEW' END, obj.relname);
    END LOOP;
    FOR obj IN SELECT p.oid, p.proname, p.prokind, pg_get_function_identity_arguments(p.oid) AS args
        FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace WHERE n.nspname = 'demo'
    LOOP
        EXECUTE format('ALTER %s demo.%I(%s) SET SCHEMA public',
            CASE obj.prokind WHEN 'p' THEN 'PROCEDURE' ELSE 'FUNCTION' END, obj.proname, obj.args);
        EXECUTE format('ALTER %s public.%I(%s) SET search_path TO public, pg_catalog',
            CASE obj.prokind WHEN 'p' THEN 'PROCEDURE' ELSE 'FUNCTION' END, obj.proname, obj.args);
    END LOOP;
END $$;
-- Refuse to remove a schema if any unsupported objects remain.
DROP SCHEMA IF EXISTS demo;
COMMIT;
