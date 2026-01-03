miniquad_add_plugin({
    name: "mobile_input",
    version: "1.0.0",

    register_plugin: function (importObject) {
        importObject.env.focus_mobile_input = function (js_object_ptr) {
            const currentValue = consume_js_object(js_object_ptr);
            const input = document.getElementById('mobile-keyboard-input');
            if (input) {
                input.value = currentValue;
                input.focus();
            }
        };

        importObject.env.get_mobile_input_value = function () {
            const input = document.getElementById('mobile-keyboard-input');
            const value = input ? input.value : "";
            return js_object(value);
        };

        importObject.env.blur_mobile_input = function () {
            const input = document.getElementById('mobile-keyboard-input');
            if (input) input.blur();
        };

      importObject.env.is_touch_device = function () {
  	const hasTouch = 'ontouchstart' in window;
   	const hasMultiTouch = navigator.maxTouchPoints > 0;
    	const isSmallScreen = window.innerWidth <= 1024;
    
    	return hasTouch || (hasMultiTouch && isSmallScreen);
      };
    }
});