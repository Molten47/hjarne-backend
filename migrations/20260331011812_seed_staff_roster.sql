-- Add migration script here
-- physicians
INSERT INTO staff (id, staff_code, first_name, last_name, role, department, specialization, license_number)
VALUES
('018e9f5b-0001-7000-8000-000000000001', 'PHY-0001', 'Sarah',   'Mitchell',  'physician', 'consultation', 'Internal Medicine',    'MD-ON-29384'),
('018e9f5b-0002-7000-8000-000000000002', 'PHY-0002', 'James',   'Okafor',    'physician', 'maternity',    'Obstetrics',           'MD-ON-38821'),
('018e9f5b-0003-7000-8000-000000000003', 'SRG-0001', 'Priya',   'Sharma',    'surgeon',   'surgery',      'General Surgery',      'MD-ON-44102'),
('018e9f5b-0004-7000-8000-000000000004', 'PHY-0003', 'Carlos',  'Reyes',     'physician', 'mental_health','Psychiatry',           'MD-ON-51903'),
('018e9f5b-0005-7000-8000-000000000005', 'NRS-0001', 'Emily',   'Tran',      'nurse',     'consultation', NULL,                   NULL),
('018e9f5b-0006-7000-8000-000000000006', 'NRS-0002', 'Michael', 'Bouchard',  'nurse',     'maternity',    NULL,                   NULL),
('018e9f5b-0007-7000-8000-000000000007', 'PHM-0001', 'Aisha',   'Johnson',   'pharmacist','pharmacy',     NULL,                   'RPH-CA-20019'),
('018e9f5b-0008-7000-8000-000000000008', 'DSK-0001', 'Tyler',   'Henderson', 'desk',      'general',      NULL,                   NULL),
('018e9f5b-0009-7000-8000-000000000009', 'DSK-0002', 'Marie',   'Leblanc',   'desk',      'general',      NULL,                   NULL);

-- auth credentials for each staff member
-- all passwords are: Staff@Password1!
INSERT INTO auth_users (entity_id, entity_type, email, password_hash, is_active)
VALUES
('018e9f5b-0001-7000-8000-000000000001', 'staff', 'sarah.mitchell@hjarne.com',  '$2b$12$j388lNbIsHTDIZi3dmWAT.Jag7nNe.lf94k8pWjueuPuUuhCJsdxC', TRUE),
('018e9f5b-0002-7000-8000-000000000002', 'staff', 'james.okafor@hjarne.com',    '$2b$12$j388lNbIsHTDIZi3dmWAT.Jag7nNe.lf94k8pWjueuPuUuhCJsdxC', TRUE),
('018e9f5b-0003-7000-8000-000000000003', 'staff', 'priya.sharma@hjarne.com',    '$2b$12$j388lNbIsHTDIZi3dmWAT.Jag7nNe.lf94k8pWjueuPuUuhCJsdxC', TRUE),
('018e9f5b-0004-7000-8000-000000000004', 'staff', 'carlos.reyes@hjarne.com',    '$2b$12$j388lNbIsHTDIZi3dmWAT.Jag7nNe.lf94k8pWjueuPuUuhCJsdxC', TRUE),
('018e9f5b-0005-7000-8000-000000000005', 'staff', 'emily.tran@hjarne.com',      '$2b$12$j388lNbIsHTDIZi3dmWAT.Jag7nNe.lf94k8pWjueuPuUuhCJsdxC', TRUE),
('018e9f5b-0006-7000-8000-000000000006', 'staff', 'michael.bouchard@hjarne.com','$2b$12$j388lNbIsHTDIZi3dmWAT.Jag7nNe.lf94k8pWjueuPuUuhCJsdxC', TRUE),
('018e9f5b-0007-7000-8000-000000000007', 'staff', 'aisha.johnson@hjarne.com',   '$2b$12$j388lNbIsHTDIZi3dmWAT.Jag7nNe.lf94k8pWjueuPuUuhCJsdxC', TRUE),
('018e9f5b-0008-7000-8000-000000000008', 'staff', 'tyler.henderson@hjarne.com', '$2b$12$j388lNbIsHTDIZi3dmWAT.Jag7nNe.lf94k8pWjueuPuUuhCJsdxC', TRUE),
('018e9f5b-0009-7000-8000-000000000009', 'staff', 'marie.leblanc@hjarne.com',   '$2b$12$j388lNbIsHTDIZi3dmWAT.Jag7nNe.lf94k8pWjueuPuUuhCJsdxC', TRUE);

-- seed more patients with North American names
INSERT INTO patients (mrn, first_name, last_name, date_of_birth, gender, blood_group, genotype, height_cm, weight_kg, nationality)
VALUES
('HJN-2026-000002', 'Sophia',   'Tremblay',  '1995-08-22', 'female', 'A+',  'AS', 165.0, 62.0, 'Canadian'),
('HJN-2026-000003', 'Marcus',   'Williams',  '1978-03-11', 'male',   'B+',  'AA', 178.0, 88.0, 'American'),
('HJN-2026-000004', 'Isabella', 'Rodriguez', '2001-11-30', 'female', 'O-',  'AA', 162.0, 57.0, 'Mexican'),
('HJN-2026-000005', 'Daniel',   'Park',      '1965-06-04', 'male',   'AB+', 'AS', 172.0, 79.0, 'American'),
('HJN-2026-000006', 'Chloe',    'Martin',    '1990-01-17', 'female', 'A-',  'AA', 168.0, 65.0, 'Canadian');