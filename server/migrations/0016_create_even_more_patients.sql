BEGIN;
WITH amina AS (
    INSERT INTO patients (legal_name, birth_date, sex)
    SELECT 'Amina Benali', DATE '1984-08-09', 'female'
    WHERE NOT EXISTS (SELECT 1 FROM patients WHERE legal_name = 'Amina Benali')
    RETURNING id
), amina_mrn AS (
    INSERT INTO patient_identifiers (patient_id, system, value, identifier_type, is_primary)
    SELECT id, 'thyme-mrn', '00044213', 'MRN', TRUE FROM amina
)
INSERT INTO patient_contacts (patient_id, type, value, preferred)
SELECT id, 'phone', '+213555112233', TRUE FROM amina;

WITH karim AS (
    INSERT INTO patients (legal_name, birth_date, sex)
    SELECT 'Karim Bensaid', DATE '1972-01-17', 'male'
    WHERE NOT EXISTS (SELECT 1 FROM patients WHERE legal_name = 'Karim Bensaid')
    RETURNING id
), karim_mrn AS (
    INSERT INTO patient_identifiers (patient_id, system, value, identifier_type, is_primary)
    SELECT id, 'thyme-mrn', '00051988', 'MRN', TRUE FROM karim
)
INSERT INTO patient_contacts (patient_id, type, value, preferred)
SELECT id, 'phone', '+213555223344', TRUE FROM karim;

WITH leila AS (
    INSERT INTO patients (legal_name, preferred_name, birth_date, sex)
    SELECT 'Leila Haddad', 'Lily', DATE '1995-12-04', 'female'
    WHERE NOT EXISTS (SELECT 1 FROM patients WHERE legal_name = 'Leila Haddad')
    RETURNING id
), leila_mrn AS (
    INSERT INTO patient_identifiers (patient_id, system, value, identifier_type, is_primary)
    SELECT id, 'thyme-mrn', '00062426', 'MRN', TRUE FROM leila
)
INSERT INTO patient_contacts (patient_id, type, value, preferred)
SELECT id, 'phone', '+213555334455', TRUE FROM leila;

WITH yacine AS (
    INSERT INTO patients (legal_name, birth_date, sex)
    SELECT 'Yacine Cherif', DATE '1988-05-30', 'male'
    WHERE NOT EXISTS (SELECT 1 FROM patients WHERE legal_name = 'Yacine Cherif')
    RETURNING id
), yacine_mrn AS (
    INSERT INTO patient_identifiers (patient_id, system, value, identifier_type, is_primary)
    SELECT id, 'thyme-mrn', '00073102', 'MRN', TRUE FROM yacine
)
INSERT INTO patient_contacts (patient_id, type, value, preferred)
SELECT id, 'phone', '+213555445566', TRUE FROM yacine;

WITH sofia AS (
    INSERT INTO patients (legal_name, preferred_name, birth_date, sex)
    SELECT 'Sofia Rahmoun', 'Soso', DATE '2001-09-12', 'female'
    WHERE NOT EXISTS (SELECT 1 FROM patients WHERE legal_name = 'Sofia Rahmoun')
    RETURNING id
), sofia_mrn AS (
    INSERT INTO patient_identifiers (patient_id, system, value, identifier_type, is_primary)
    SELECT id, 'thyme-mrn', '00084577', 'MRN', TRUE FROM sofia
)
INSERT INTO patient_contacts (patient_id, type, value, preferred)
SELECT id, 'phone', '+213555556677', TRUE FROM sofia;

WITH ahmed AS (
    INSERT INTO patients (legal_name, preferred_name, birth_date, sex)
    SELECT 'Ahmed Ali', 'Ahmed', DATE '1990-03-15', 'male'
    WHERE NOT EXISTS (SELECT 1 FROM patients WHERE legal_name = 'Ahmed Ali')
    RETURNING id
), ahmed_mrn AS (
    INSERT INTO patient_identifiers (patient_id, system, value, identifier_type, is_primary)
    SELECT id, 'thyme-mrn', '00095688', 'MRN', TRUE FROM ahmed
)
INSERT INTO patient_contacts (patient_id, type, value, preferred)
SELECT id, 'phone', '+213555667788', TRUE FROM ahmed;


WITH sajed AS (
    INSERT INTO patients (legal_name, preferred_name, birth_date, sex)
    SELECT 'Sajed Al-Din', 'Sajed', DATE '1985-07-20', 'male'
    WHERE NOT EXISTS (SELECT 1 FROM patients WHERE legal_name = 'Sajed Al-Din')
    RETURNING id
), sajed_mrn AS (
    INSERT INTO patient_identifiers (patient_id, system, value, identifier_type, is_primary)
    SELECT id, 'thyme-mrn', '00084577', 'MRN', TRUE FROM sajed
)
INSERT INTO patient_contacts (patient_id, type, value, preferred)
SELECT id, 'phone', '+213555556677', TRUE FROM sajed;


WITH yaqub AS (
    INSERT INTO patients (legal_name, preferred_name, birth_date, sex)
    SELECT 'Yaqub Benchouri', 'Yaqub', DATE '1995-05-10', 'male'
    WHERE NOT EXISTS (SELECT 1 FROM patients WHERE legal_name = 'Yaqub Benchouri')
    RETURNING id
), yaqub_mrn AS (
    INSERT INTO patient_identifiers (patient_id, system, value, identifier_type, is_primary)
    SELECT id, 'thyme-mrn', '00084577', 'MRN', TRUE FROM yaqub
)
INSERT INTO patient_contacts (patient_id, type, value, preferred)
SELECT id, 'phone', '+213532455667', TRUE FROM yaqub;


WITH hakim AS (
    INSERT INTO patients (legal_name, preferred_name, birth_date, sex)
    SELECT 'Hakim Slimani', 'Hakim', DATE '1980-12-25', 'male'
    WHERE NOT EXISTS (SELECT 1 FROM patients WHERE legal_name = 'Hakim Slimani')
    RETURNING id
), hakim_mrn AS (
    INSERT INTO patient_identifiers (patient_id, system, value, identifier_type, is_primary)
    SELECT id, 'thyme-mrn', '00084577', 'MRN', TRUE FROM hakim
)
INSERT INTO patient_contacts (patient_id, type, value, preferred)
SELECT id, 'phone', '+213555493677', TRUE FROM hakim;


WITH radhya AS (
    INSERT INTO patients (legal_name, preferred_name, birth_date, sex)
    SELECT 'Radhya Abdou', 'Soso', DATE '2001-09-12', 'female'
    WHERE NOT EXISTS (SELECT 1 FROM patients WHERE legal_name = 'Radhya Abdou')
    RETURNING id
), radhya_mrn AS (
    INSERT INTO patient_identifiers (patient_id, system, value, identifier_type, is_primary)
    SELECT id, 'thyme-mrn', '00084577', 'MRN', TRUE FROM radhya
)
INSERT INTO patient_contacts (patient_id, type, value, preferred)
SELECT id, 'phone', '+213555432577', TRUE FROM radhya;

COMMIT;
