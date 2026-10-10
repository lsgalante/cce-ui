use super::*;

#[test]
fn runs_are_reordered_as_the_bidi_algorithm_draws_them() {
    assert_eq!(visual_run_order(&["say ", "שלום", " עולם", " now"], false), [0, 2, 1, 3]);
    assert_eq!(visual_run_order(&["שלום ", "bold", " and", " עולם"], true), [3, 1, 2, 0], "English keeps its order inside");
    assert_eq!(visual_run_order(&["a", "b", "c"], false), [0, 1, 2]);
    assert_eq!(visual_run_order(&["א", "ב"], true), [1, 0]);
    assert_eq!(visual_run_order(&[" ", "123"], true), [1, 0], "neutral runs sit at the paragraph's level");
    assert!(paragraph_rtl("  «שלום» hello") && !paragraph_rtl("hello שלום") && !paragraph_rtl("123 ..."));
}
