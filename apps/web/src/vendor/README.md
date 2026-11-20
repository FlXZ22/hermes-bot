`grok-face-runtime.js` is adapted from mikeg0/grokbot-face, commit
26fdd2933ff7bda1705ab683515282a0328a8bb7:
https://github.com/mikeg0/grokbot-face/blob/26fdd2933ff7bda1705ab683515282a0328a8bb7/face-runtime.js

The global factory was converted to an ES module. The React host lazy-loads it,
limits advancement to 30 fps, pauses during scrolling, and stops when offscreen,
when the document is hidden, or when reduced motion is requested. Each mount
owns its renderer, observers, listeners, frame callback, and cleanup.
