CREATE TABLE
    roles (
        id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
        name TEXT NOT NULL,
        description TEXT,
        created_at TIMESTAMPTZ NOT NULL DEFAULT now (),
        CONSTRAINT roles_name_unique UNIQUE (name)
    );

INSERT INTO
    roles (name, description)
VALUES
    ('doctor', 'Medical doctor'),
    (
        'receptionist',
        'Reception and administrative staff'
    ),
    ('administrator', 'System administrator');