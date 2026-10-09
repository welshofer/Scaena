// The pointer on CI's Mac (PLAN 3.27): a click, a double click, a right click, or a drag, posted as
// the mouse posts them, at screen points from the top left, where System Events puts a window. The
// canvas's gestures take these as a person's; System Events' `click at` presses an accessibility
// element instead, and the canvas has none to press.
//
//   osascript -l JavaScript apps/mac/pointer.js click X Y
//   osascript -l JavaScript apps/mac/pointer.js double X Y
//   osascript -l JavaScript apps/mac/pointer.js right X Y
//   osascript -l JavaScript apps/mac/pointer.js drag X Y TO_X TO_Y
ObjC.import("CoreGraphics");

// CGEventType, CGMouseButton, CGEventTapLocation, and CGEventField, as CoreGraphics numbers them.
const LEFT_DOWN = 1;
const LEFT_UP = 2;
const RIGHT_DOWN = 3;
const RIGHT_UP = 4;
const MOVED = 5;
const LEFT_DRAGGED = 6;
const LEFT = 0;
const RIGHT = 1;
const HID = 0;
const CLICK_STATE = 1;

function run(argv) {
  const [how, ...rest] = argv;
  const [x, y, toX, toY] = rest.map(Number);
  const post = (type, px, py, button, clicks) => {
    const e = $.CGEventCreateMouseEvent(null, type, $.CGPointMake(px, py), button);
    if (clicks) $.CGEventSetIntegerValueField(e, CLICK_STATE, clicks);
    $.CGEventPost(HID, e);
    delay(0.05);
  };
  post(MOVED, x, y, LEFT);
  delay(0.3);
  switch (how) {
    case "click":
      post(LEFT_DOWN, x, y, LEFT, 1);
      post(LEFT_UP, x, y, LEFT, 1);
      break;
    case "double":
      post(LEFT_DOWN, x, y, LEFT, 1);
      post(LEFT_UP, x, y, LEFT, 1);
      post(LEFT_DOWN, x, y, LEFT, 2);
      post(LEFT_UP, x, y, LEFT, 2);
      break;
    case "right":
      post(RIGHT_DOWN, x, y, RIGHT, 1);
      post(RIGHT_UP, x, y, RIGHT, 1);
      break;
    case "drag": {
      post(LEFT_DOWN, x, y, LEFT, 1);
      const steps = 16;
      for (let i = 1; i <= steps; i++) {
        post(LEFT_DRAGGED, x + ((toX - x) * i) / steps, y + ((toY - y) * i) / steps, LEFT);
        delay(0.03);
      }
      // Held still a moment where it lands, as a hand lets go.
      delay(0.4);
      post(LEFT_UP, toX, toY, LEFT, 1);
      break;
    }
    default:
      throw new Error(`no such gesture: ${how}`);
  }
  return `${how} at ${rest.join(", ")}`;
}
