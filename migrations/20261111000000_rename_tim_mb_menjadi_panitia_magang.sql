-- Tim MB is now called Panitia Magang. Only names change: the rows, the foreign key from
-- log_status.id_pengubah, and how the table is used stay the same.
ALTER TABLE tim_mb RENAME TO panitia_magang;

-- Constraints (and the index behind the primary key) carry the old table name in theirs.
DO $$
DECLARE
    lama text;
BEGIN
    FOR lama IN
        SELECT conname FROM pg_constraint
        WHERE conrelid = 'panitia_magang'::regclass AND conname LIKE 'tim\_mb\_%'
    LOOP
        EXECUTE format(
            'ALTER TABLE panitia_magang RENAME CONSTRAINT %I TO %I',
            lama,
            'panitia_magang_' || substr(lama, length('tim_mb_') + 1)
        );
    END LOOP;
END
$$;
