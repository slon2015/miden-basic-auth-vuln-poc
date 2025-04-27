use std::u64;

use miden_lib::{note::utils::build_p2id_recipient, transaction::TransactionKernel};
use miden_objects::{account::AccountId, asset::FungibleAsset, crypto::rand::{FeltRng, RpoRandomCoin}, note::{Note, NoteAssets, NoteExecutionHint, NoteInputs, NoteMetadata, NoteRecipient, NoteScript, NoteTag, NoteType}, testing::account_id::ACCOUNT_ID_SENDER, Felt, Word, ZERO};
use miden_tx::{testing::{Auth, MockChain}, utils::word_to_masm_push_string};

fn get_note_with_fungible_asset_and_script(
    fungible_asset: FungibleAsset,
    note_script: &str,
) -> Note {
    use miden_objects::note::NoteExecutionHint;

    let assembler = TransactionKernel::assembler().with_debug_mode(true);
    let note_script = NoteScript::compile(note_script, assembler).unwrap();
    const SERIAL_NUM: Word = [Felt::new(1), Felt::new(2), Felt::new(3), Felt::new(4)];
    let sender_id = AccountId::try_from(ACCOUNT_ID_SENDER).unwrap();

    let vault = NoteAssets::new(vec![fungible_asset.into()]).unwrap();
    let metadata =
        NoteMetadata::new(sender_id, NoteType::Public, 1.into(), NoteExecutionHint::Always, ZERO)
            .unwrap();
    let inputs = NoteInputs::new(vec![]).unwrap();
    let recipient = NoteRecipient::new(SERIAL_NUM, note_script, inputs);

    Note::new(vault, metadata, recipient)
}

#[test]
fn burn_and_mint_without_authentication() {

    let mut rng = RpoRandomCoin::new([Felt::new(1); 4]);

    let mut mock_chain = MockChain::new();
    let faucet = mock_chain.add_existing_faucet(Auth::BasicAuth, "TST", u64::MAX, Some(1));
    let mut faucet_account = faucet.account().clone();
    let receiver= mock_chain.add_existing_wallet(Auth::BasicAuth, Vec::new());

    let fungible_asset = FungibleAsset::new(faucet.account().id(), 1).unwrap();

    let serial_num = rng.draw_word();
    let recipient = build_p2id_recipient(receiver.id(), serial_num).unwrap();

    let recipient_hash = recipient.digest();
    let aux = Felt::new(27);
    let tag = NoteTag::for_local_use_case(123, 0).unwrap().inner();
    let amount = Felt::new(250);
    let note_execution_hint = NoteExecutionHint::Always;
    let note_type = NoteType::Private;

    // need to create a note with the fungible asset to be burned
    let note_script = format!("
        # burn the asset
        begin
            dropw

            # pad the stack before call
            padw padw padw padw
            # => [pad(16)]

            exec.::miden::note::get_assets drop
            mem_loadw
            # => [ASSET, pad(12)]

            call.::miden::contracts::faucets::basic_fungible::burn
            dropw dropw dropw dropw

            push.{recipient}
            push.{note_execution_hint}
            push.{note_type}
            push.{aux}
            push.{tag}
            push.{amount}
            # => [amount, tag, aux, note_type, execution_hint, RECIPIENT, pad(7)]

            call.::miden::contracts::faucets::basic_fungible::distribute
            # => [note_idx, pad(15)]

            # truncate the stack
            dropw dropw dropw dropw
        end", 
        note_type = note_type as u8,
        recipient = word_to_masm_push_string(&recipient_hash),
        aux = aux,
        tag = tag,
        note_execution_hint = Felt::from(note_execution_hint)
    );

    let note = get_note_with_fungible_asset_and_script(fungible_asset, note_script.as_str());

    mock_chain.add_pending_note(note.clone());
    mock_chain.seal_next_block();

    let executed_transaction = mock_chain
        .build_tx_context(faucet.account().id(), &[note.id()], &[])
        .build()
        .execute();

    assert!(executed_transaction.is_ok());
    let executed_transaction = executed_transaction.unwrap();
    assert!(executed_transaction.output_notes().num_notes() == 1);

    faucet_account.apply_delta(executed_transaction.account_delta()).unwrap();
    assert_eq!(faucet_account.storage().get_item(0).unwrap()[3], amount.clone());
}