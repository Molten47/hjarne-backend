-- seed admin staff member
INSERT INTO staff (id, staff_code, first_name, last_name, role, department, is_active)
VALUES (
    '018e9f5a-1234-7000-8000-000000000001',
    'ADMIN-001',
    'Marcus',
    'Webb',
    'admin',
    'general',
    TRUE
);

-- seed auth user for admin
-- password is: AdminPassword123!
INSERT INTO auth_users (entity_id, entity_type, email, password_hash, is_active)
VALUES (
    '018e9f5a-1234-7000-8000-000000000001',
    'staff',
    'marcus.webb@hjarne.com',
    '$2b$12$j388lNbIsHTDIZi3dmWAT.Jag7nNe.lf94k8pWjueuPuUuhCJsdxC',
    TRUE
);