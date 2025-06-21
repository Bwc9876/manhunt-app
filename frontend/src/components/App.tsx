import React from "react";
import useSWR from "swr";
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

export default function App() {
    const { data: screen, mutate } = useSWR(
        "fetch-screen",
        async () => {
            return unwrapResult(await commands.getCurrentScreen());
        },
        { suspense: true, dedupingInterval: 100 }
    );

    useTauriEvent("changeScreen", (newScreen) => {
        mutate(newScreen);
    });

    return (
        <>
            <h1>Screen: {screen}</h1>
            <ScreenRouter screen={screen} />
        </>
    );
}
