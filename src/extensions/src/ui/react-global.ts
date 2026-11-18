// The design system bundle reads React from `window.React` (it was written for a
// page with React loaded globally). This module runs first and provides it.
import * as React from "react";

(window as unknown as { React: typeof React }).React = React;
