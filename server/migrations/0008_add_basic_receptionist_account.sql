BEGIN;

INSERT INTO users (username, display_name, password_hash)
VALUES (
  'receptionist',
  'Receptionist',
  'recep'
)
ON CONFLICT (username) DO NOTHING;

INSERT INTO roles (name)
VALUES ('receptionist')
ON CONFLICT (name) DO NOTHING;

INSERT INTO user_roles (user_id, role_id)
SELECT u.id, r.id
FROM users u
JOIN roles r ON r.name = 'receptionist'
WHERE u.username = 'receptionist'
ON CONFLICT (user_id, role_id) DO NOTHING;

COMMIT;
