//! Fender FUSE `.fuse` XML import and export.

use scx2::fuse;
use scx2::protocol::{Kind, OFF_PARAMS};

const SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<Preset name="Tweed &amp; Twin" amplifier="Mustang" productId="1">
  <Amplifier>
    <Module ID="3" POS="0" BypassState="0">
      <Param ControlIndex="0">43690</Param>
      <Param ControlIndex="1">38550</Param>
      <Param ControlIndex="4">30326</Param>
      <Param ControlIndex="5">33023</Param>
      <Param ControlIndex="6">255</Param>
    </Module>
  </Amplifier>
  <FX>
    <Stompbox>
      <Module ID="19" POS="3" BypassState="0">
        <Param ControlIndex="0">32896</Param>
        <Param ControlIndex="1">32896</Param>
        <Param ControlIndex="2">32896</Param>
      </Module>
    </Stompbox>
    <Modulation />
    <Delay>
      <Module ID="38" POS="6" BypassState="1">
        <Param ControlIndex="0">32125</Param>
        <Param ControlIndex="1">7188</Param>
      </Module>
    </Delay>
    <Reverb>
      <Module ID="51" POS="5" BypassState="0">
        <Param ControlIndex="0">32896</Param>
      </Module>
    </Reverb>
  </FX>
  <Info name="Tweed &amp; Twin"></Info>
</Preset>
"#;

#[test]
fn a_fuse_file_becomes_a_live_preset() {
    let preset = fuse::parse(SAMPLE).expect("parse sample");

    assert_eq!(preset.bank, 0);
    assert_eq!(preset.index, 0);
    assert_eq!(preset.name, "Tweed & Twin");
    assert_eq!(preset.amp_model(), Some(0x0053));

    let amp = preset.block(Kind::Amp).unwrap();
    assert_eq!(amp.byte(44), 0x03, "the voice follows the amp model");
    assert_eq!(amp.byte(OFF_PARAMS), 0xaa);
    assert_eq!(amp.byte(OFF_PARAMS + 1), 0x96);

    let stomp = preset.block(Kind::Stomp).unwrap();
    assert_eq!(stomp.model(), Some(0x003c));
    assert_eq!(stomp.slot(), Some(3));
    assert_eq!(stomp.byte(OFF_PARAMS), 0x80);

    let delay = preset.block(Kind::Delay).unwrap();
    assert_eq!(delay.model(), Some(0x002b));
    assert_eq!(delay.slot(), Some(6));
    assert_eq!(delay.byte(22), 1, "BypassState=1 is stored at offset 22");
    assert_eq!(delay.byte(OFF_PARAMS), 0x7d);
}

#[test]
fn a_parsed_preset_exports_back_to_fuse_xml() {
    let preset = fuse::parse(SAMPLE).expect("parse sample");
    let xml = fuse::export(&preset).expect("export");
    assert!(xml.contains("<Preset name=\"Tweed &amp; Twin\""));
    assert!(xml.contains("<Module ID=\"3\""));
    assert!(xml.contains("<Module ID=\"19\""));
    assert!(xml.contains("<Module ID=\"38\""));

    let again = fuse::parse(&xml).expect("reparse");
    assert_eq!(again.name, preset.name);
    for kind in [Kind::Amp, Kind::Stomp, Kind::Delay, Kind::Reverb] {
        let before = preset.block(kind).unwrap();
        let after = again.block(kind).unwrap();
        assert_eq!(after.model(), before.model(), "{kind:?} model");
        assert_eq!(after.slot(), before.slot(), "{kind:?} slot");
        assert_eq!(after.params(), before.params(), "{kind:?} parameter bytes");
    }
}

#[test]
fn an_empty_effect_group_is_kept_empty() {
    let xml = r#"<?xml version="1.0"?>
<Preset name="Amp only">
  <Amplifier><Module ID="0" POS="0" BypassState="0"><Param ControlIndex="0">43690</Param></Module></Amplifier>
  <FX><Stompbox /><Modulation /><Delay /><Reverb /></FX>
</Preset>"#;
    let preset = fuse::parse(xml).expect("parse");
    let out = fuse::export(&preset).expect("export");
    assert!(out.contains("<Stompbox />"));
    assert!(out.contains("<Reverb />"));
    assert!(!out.contains("<Module ID=\"0\" POS=\"3\""));
}

#[test]
fn a_missing_amplifier_is_rejected() {
    let error = fuse::parse("<Preset name=\"x\"><FX /></Preset>").unwrap_err();
    assert!(
        format!("{error}").contains("Amplifier"),
        "unexpected error: {error}"
    );
}
