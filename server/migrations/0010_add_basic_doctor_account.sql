BEGIN;

INSERT INTO users (username, display_name, password_hash)
VALUES (
  'doctor',
  'Doctor',
  'doktour'
)
ON CONFLICT (username) DO NOTHING;

-- 'doctor' already exists from 0002_create_roles.sql; this just links
-- the seeded account to it, matching 0008's pattern for receptionist.
INSERT INTO user_roles (user_id, role_id)
SELECT u.id, r.id
FROM users u
JOIN roles r ON r.name = 'doctor'
WHERE u.username = 'doctor'
ON CONFLICT (user_id, role_id) DO NOTHING;

COMMIT;
