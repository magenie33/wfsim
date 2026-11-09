use super::*;

#[test]
fn reason_text_is_shared_without_changing_admissions() {
    let mut shared = 0;
    let mut parameterized = 0;
    let mut plain = 0;
    for weapon in all() {
        assert_eq!(weapon.unmodeled.len(), weapon.unmodeled_parts.len(), "{}", weapon.id);
        for (text, part) in weapon.unmodeled.iter().zip(&weapon.unmodeled_parts) {
            assert_eq!(text, &part.text, "{}: English fields agree", weapon.id);
            let Some(id) = part.reason.as_deref() else {
                plain += 1;
                assert!(part.template.is_none() && part.params.is_empty());
                continue;
            };
            let template = reasons().get(id).unwrap();
            assert_eq!(part.template.as_deref(), Some(template.as_str()));
            assert_eq!(text.as_ref(), fill_template(template, &part.params));
            assert!(std::ptr::eq(part.template.as_deref().unwrap(), template.as_str()),
                "{}: the reason template must not be copied", weapon.id);
            if text.as_ref() == template.as_str() {
                shared += 1;
                assert!(std::ptr::eq(text.as_ref(), template.as_str()), "{}: English must share the reason", weapon.id);
                assert!(std::ptr::eq(part.text.as_ref(), template.as_str()), "{}: structured English must share the reason", weapon.id);
            } else {
                parameterized += 1;
            }
        }
    }
    assert!(shared > all().len(), "repeated reasons must share their storage");
    assert!(parameterized > 0 && plain > 0, "retain parameterized and verbatim admissions");
}
