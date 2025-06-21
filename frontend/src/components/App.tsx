import React from "react";
import { AppScreen, commands } from "@/bindings";
import { useTauriEvent } from "@/lib/hooks";
import { unwrapResult } from "@/lib/result";
import SetupScreen from "./SetupScreen";
import MenuScreen from "./MenuScreen";
import LobbyScreen from "./LobbyScreen";
import GameScreen from "./GameScreen";

function ScreenRouter({ screen }: { screen: AppScreen }) {
    switch (screen) {
        case "Setup":
            return <SetupScreen />;
        case "Menu":
            return <MenuScreen />;
        case "Lobby":
            return <LobbyScreen />;
        case "Game":
            return <GameScreen />;
        default:
            return <p>???</p>;
    }
}

const startingScreen = unwrapResult(await commands.getCurrentScreen());

export default function App() {
    const [screen, setScreen] = React.useState<AppScreen>(startingScreen);

    useTauriEvent("changeScreen", setScreen);

    return (
        <>
            <h1>Screen: {screen}</h1>
            <ScreenRouter screen={screen} />
        </>
    );
}
