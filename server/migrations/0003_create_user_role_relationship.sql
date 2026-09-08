CREATE TABLE
    user_roles (
        user_id UUID NOT NULL,
        role_id UUID NOT NULL,
        assigned_at TIMESTAMPTZ NOT NULL DEFAULT now (),
        PRIMARY KEY (user_id, role_id),
        CONSTRAINT user_roles_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
        CONSTRAINT user_roles_role_fk FOREIGN KEY (role_id) REFERENCES roles (id) ON DELETE RESTRICT
    );

CREATE INDEX idx_user_roles_role_id ON user_roles (role_id);