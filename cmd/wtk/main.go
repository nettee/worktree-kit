package main

import (
	"context"
	"fmt"
	"os"

	"github.com/nettee/worktree-kit/internal/cli"
)

func main() {
	if err := cli.Execute(context.Background(), os.Args[1:]); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
