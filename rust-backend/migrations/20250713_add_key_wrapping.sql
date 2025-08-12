ALTER TABLE api_keys
    ADD COLUMN encrypted_data_key BYTEA NOT NULL,
    ADD COLUMN nonce_key BYTEA NOT NULL,
    ADD COLUMN nonce_secret BYTEA NOT NULL,
    ADD COLUMN nonce_passphrase BYTEA;

-- Back-fill existing rows with dummy placeholders so the NOT NULL passes
UPDATE api_keys
SET encrypted_data_key = '\\x00', nonce_key='\\x00', nonce_secret='\\x00'
WHERE encrypted_data_key IS NULL;
