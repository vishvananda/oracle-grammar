use crate::style::Model;
use crate::tokens::Token;

#[test]
fn precomputed_print_weights_keep_backoff_and_learned_scores() {
    let mut model = Model::parse("$ $ x 2\n$ $ x 4\n$ $ y 6\nq x z 8\nq x x 2\n");
    let x = Token::Word("x".into());
    let start = (4.0f32 / 12.0).ln();
    assert_eq!(
        model.score(std::slice::from_ref(&x), true).to_bits(),
        start.to_bits()
    );

    let x_unigram = 2.0 * 0.4f32.ln() + (8.0f32 / 22.0).ln();
    let z_bigram = 0.4f32.ln() + (8.0f32 / 10.0).ln();
    let unseen_then_known = [Token::Mark('!'), x.clone(), Token::Word("z".into())];
    assert_eq!(
        model.score(&unseen_then_known, false).to_bits(),
        (-14.0 + x_unigram + z_bigram).to_bits()
    );

    model.learn("$ $ x 0.25\n");
    assert_eq!(model.score(&[x], true).to_bits(), (start + 0.25).to_bits());
}
