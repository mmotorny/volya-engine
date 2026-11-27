odeharness.exe: the physics library comparison of volya-engine
(https://github.com/mmotorny/volya-engine, tests/odeharness/).

It loads the game's bin\ode.dll and runs synthetic scenarios of our own
through the library's exported functions, writing what the library
computes. It reads nothing else of the game and changes nothing.

Run, in a folder of your choice:

  odeharness.exe "<game folder>\bin" results

It writes results\<scenario>.bin and results\info.txt and prints a line per
scenario. It exits with 0, or with 1 and a message if the library or one of
its functions is missing.

Then compare them with our build's outputs (ours\, from our build on the
Mac):

  odeharness.exe compare ours results

It prints, per scenario, identical, or how many words differ and the first.
The results are the shipped library's: keep them on this PC.
