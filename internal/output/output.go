package output

import (
	"fmt"
	"io"
	"strings"

	"github.com/fatih/color"
)

type Renderer struct{ Out io.Writer }

func (r Renderer) Info(format string, args ...any) {
	fmt.Fprintf(r.Out, color.New(color.FgCyan, color.Bold).Sprintf("==> ")+format+"\n", args...)
}
func (r Renderer) Success(format string, args ...any) {
	fmt.Fprintf(r.Out, color.New(color.FgGreen, color.Bold).Sprintf("✓ ")+format+"\n", args...)
}
func (r Renderer) Warn(format string, args ...any) {
	fmt.Fprintf(r.Out, color.New(color.FgYellow, color.Bold).Sprintf("! ")+format+"\n", args...)
}
func (r Renderer) Git(dir string, args ...string) {
	fmt.Fprintf(r.Out, "$ git -C %s %s\n", dir, strings.Join(args, " "))
}
