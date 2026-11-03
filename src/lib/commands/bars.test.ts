// The colour a button's command numbers its lines in: the colour the button shows.
import { describe, expect, it } from "vitest";
import { buttonColour, type BarButton } from "./bars";

const button = (icon: string, color = ""): BarButton => ({ label: "ls -la", tooltip: "", color, icon, action: "SendString", text: "ls -la\\n" });

describe("buttonColour", () => {
  it("takes an icon's chosen colour", () => {
    expect(buttonColour(button("switch#A371F7"))).toBe("#A371F7");
  });

  it("takes an icon's own colour when none is chosen", () => {
    expect(buttonColour(button("switch"))).toBe("#2E7D32");
  });

  it("takes none for a custom picture without a colour, and the chosen one with", () => {
    expect(buttonColour(button("custom:logo"))).toBe("");
    expect(buttonColour(button("custom:logo#FF0000"))).toBe("#FF0000");
  });

  it("takes the circle's colour without an icon, and none for neither", () => {
    expect(buttonColour(button("", "#3FB950"))).toBe("#3FB950");
    expect(buttonColour(button(""))).toBe("");
  });
});
