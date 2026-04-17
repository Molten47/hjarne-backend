use uuid::Uuid;

// every redis key in the system is defined here
// one place to look, one place to change
// format mirrors what we designed in the API surface planning

pub fn user_me(user_id: Uuid) -> String {
    format!("user:{user_id}:me")
}

pub fn patient(patient_id: Uuid) -> String {
    format!("patient:{patient_id}")
}

pub fn patient_history(patient_id: Uuid) -> String {
    format!("patient:{patient_id}:history")
}

pub fn patient_search(hash: &str) -> String {
    format!("patients:search:{hash}")
}

pub fn case_file(case_id: Uuid) -> String {
    format!("case:{case_id}")
}

pub fn appointments_today(user_id: Uuid) -> String {
    format!("appts:today:{user_id}")
}

pub fn appointments_date_physician(date: &str, physician_id: Uuid) -> String {
    format!("appts:{date}:{physician_id}")
}

pub fn pharmacy_queue() -> String {
    "pharmacy:queue".to_string()
}

pub fn pharmacy_stock() -> String {
    "pharmacy:stock".to_string()
}

pub fn drug_search(query: &str) -> String {
    format!("drugs:search:{query}")
}

pub fn admin_stats(period: &str) -> String {
    format!("admin:stats:{period}")
}

pub fn staff_list(dept: &str, role: &str) -> String {
    format!("staff:list:{dept}:{role}")
}

pub fn patient_list_admin(cursor: &str, limit: i64) -> String {
    format!("patients:list:admin:{cursor}:{limit}")
}

pub fn patient_list_staff(staff_id: Uuid, cursor: &str, limit: i64) -> String {
    format!("patients:list:staff:{staff_id}:{cursor}:{limit}")
}

pub fn patient_search_staff(staff_id: Uuid, hash: &str) -> String {
    format!("patients:search:staff:{staff_id}:{hash}")
}

pub fn refresh_token(user_id: Uuid) -> String {
    format!("user:{user_id}:refresh")
}
pub fn refresh_token_family(family_id: Uuid) -> String {
    format!("refresh:family:{family_id}")
}
