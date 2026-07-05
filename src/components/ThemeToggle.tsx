"use client";

/* eslint-disable react-hooks/set-state-in-effect --
 * We must read the theme from localStorage on mount and update state.
 */

import { useEffect, useState } from "react";
import { Moon, Sun } from "lucide-react";
import { Button } from "@/components/ui/button";

export function ThemeToggle() {
  const [theme, setTheme] = useState<"light" | "dark" | null>(null);

  // Initialize theme on mount
  useEffect(() => {
    const storedTheme = localStorage.getItem("theme") as "light" | "dark" | null;
    const defaultDark = storedTheme === null || storedTheme === "dark";
    const currentTheme = defaultDark ? "dark" : "light";
    
    setTheme(currentTheme);
    if (currentTheme === "dark") {
      document.documentElement.classList.add("dark");
    } else {
      document.documentElement.classList.remove("dark");
    }
  }, []);

  const toggleTheme = () => {
    if (!theme) return;
    const nextTheme = theme === "dark" ? "light" : "dark";
    setTheme(nextTheme);
    localStorage.setItem("theme", nextTheme);
    
    if (nextTheme === "dark") {
      document.documentElement.classList.add("dark");
    } else {
      document.documentElement.classList.remove("dark");
    }
  };

  if (!theme) return null;

  return (
    <div className="fixed bottom-4 right-4 z-50 tauri-no-drag">
      <Button
        variant="outline"
        size="icon"
        onClick={toggleTheme}
        className="h-10 w-10 rounded-full border border-border bg-card/85 text-foreground shadow-lg backdrop-blur-md transition-all duration-300 hover:scale-105 hover:bg-card hover:border-[#c15f3c]/40 focus-visible:ring-2 focus-visible:ring-[#c15f3c] cursor-pointer"
        title={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
      >
        {theme === "dark" ? (
          <Sun className="size-[1.2rem] text-[#c15f3c] transition-all duration-300 rotate-0 scale-100" />
        ) : (
          <Moon className="size-[1.2rem] text-[#c15f3c] transition-all duration-300 rotate-0 scale-100" />
        )}
      </Button>
    </div>
  );
}
