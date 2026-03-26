function (event) {
  function setPosition (newPosition) {
    var led = event.icon.find ('[mod-role=waveform-led]');
    var oldPosition = led.data('xBgPosition');
    if (oldPosition === newPosition) {
      return;
    }
    led.css({ backgroundPosition: newPosition});
    led.data ('xBgPosition', newPosition);
  }

  if (event.type == 'change') {
    if (event.symbol === "waveForm") {
      if (event.value === 2 || event.value === 3) {
        setPosition("left center");
      } else {
        setPosition("right center");
      }
    }
  }
}
