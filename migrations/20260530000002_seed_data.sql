-- Seed data for development/testing
-- Creates a test business with an API key

-- Test business
INSERT INTO businesses (id, name, created_at, updated_at) VALUES
    ('a1b2c3d4-e5f6-7890-abcd-ef1234567890', 'Test Business', NOW(), NOW());

-- API key: dodo_test_key_1234567890abcdef
-- SHA-256 hash of 'dodo_test_key_1234567890abcdef'
INSERT INTO api_keys (id, business_id, key_prefix, key_hash, created_at) VALUES
    ('b1c2d3e4-f5a6-7890-bcde-f12345678901',
     'a1b2c3d4-e5f6-7890-abcd-ef1234567890',
     'dodo_tes',
     '6bb3601f2b8cc2959034b7b5b954f1de67d73ca771afe57c5acca1020c90e032',
     NOW());
