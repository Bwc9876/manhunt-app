import React from "react";
import { commands } from "@/bindings";
import { sharedSwrConfig, useTauriEvent } from "@/lib/hooks";
import useSWR from "swr";

export default function LobbyScreen() {
    const { data: lobbyState, mutate } = useSWR(
        "fetch-lobby-state",
        commands.getLobbyState,
        sharedSwrConfig
    );

    useTauriEvent("lobbyStateUpdate", () => {
        mutate();
    });

    const setSeeker = async (seeker: boolean) => {
        await commands.switchTeams(seeker);
    };

    const startGame = async () => {
        await commands.hostStartGame();
    };

    const quit = async () => {
        await commands.quitToMenu();
    };

    if (lobbyState.self_id === null) {
        return <h2>Connecting to Lobby...</h2>;
    }

    return (
        <>
            <h2>Join Code: {lobbyState.join_code}</h2>

            {lobbyState.is_host && <button onClick={startGame}>Start Game</button>}

            <button onClick={() => setSeeker(true)}>Become Seeker</button>
            <button onClick={() => setSeeker(false)}>Become Hider</button>

            <h3>Seekers</h3>
            <ul>
                {Object.keys(lobbyState.teams)
                    .filter((k) => lobbyState.teams[k])
                    .map((key) => (
                        <li key={key}>{lobbyState.profiles[key]?.display_name ?? key}</li>
                    ))}
            </ul>
            <h3>Hiders</h3>
            <ul>
                {Object.keys(lobbyState.teams)
                    .filter((k) => !lobbyState.teams[k])
                    .map((key) => (
                        <li key={key}>{lobbyState.profiles[key]?.display_name ?? key}</li>
                    ))}
            </ul>
            <button onClick={quit}>Quit to Menu</button>
        </>
    );
}
