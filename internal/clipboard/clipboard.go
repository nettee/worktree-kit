package clipboard

import atotto "github.com/atotto/clipboard"

type Clipboard interface{ WriteText(string) error }

type System struct{}

func (System) WriteText(s string) error { return atotto.WriteAll(s) }

type Disabled struct{}

func (Disabled) WriteText(string) error { return nil }
