import { commands, GameSettings } from "@/bindings";
import { sharedSwrConfig } from "@/lib/hooks";
import React, { Dispatch, SetStateAction } from "react";
import useSWR from "swr";
import NavButton from "./NavButton";
import JoinLobby from "./JoinLobby";
import { IoAddOutline, IoArrowForward, IoAccessibilityOutline, IoCalendarClearOutline } from "react-icons/io5";

// Temp settings for now.
const settings: GameSettings = {
    random_seed: 21341234,
    hiding_time_seconds: 10,
    ping_start: "Instant",
    ping_minutes_interval: 1,
    powerup_start: "Instant",
    powerup_chance: 60,
    powerup_minutes_cooldown: 1,
    powerup_locations: [
        {
            lat: 0,
            long: 0,
            heading: null
        }
    ]
};

export enum MenuState {
    Join,
    Create,
    Profile,
    History
}

export function MenuRouter({ state }: { state: MenuState }) {
    switch (state) {
        case MenuState.Join:
            return <JoinLobby settings={settings} />;

        case MenuState.Create:
            return <div>Create</div>;

        case MenuState.Profile:
            return <div>Profile</div>;

        case MenuState.History:
            return <div>History</div>;
    }
    return <></>;
}

function NavBar({
    state,
    setState
}: {
    state: MenuState;
    setState: Dispatch<SetStateAction<MenuState>>;
}) {
    return (
        <div className="w-full h-1/8 flex flex-row border-t-1 border-gray-300 fixed bottom-0">
            <NavButton current={state} setCurrent={setState} target={MenuState.Join}>
                <IoArrowForward className="size-1/3"/>
                Join
            </NavButton>

            <NavButton current={state} setCurrent={setState} target={MenuState.Create}>
                <IoAddOutline className="size-1/3"/>
                Create
            </NavButton>

            <NavButton current={state} setCurrent={setState} target={MenuState.Profile}>
                <IoAccessibilityOutline className="size-1/3"/>
                Profile
            </NavButton>

            <NavButton current={state} setCurrent={setState} target={MenuState.History}>
                <IoCalendarClearOutline className="size-1/3"/>
                History
            </NavButton>
        </div>
    );
}

export default function MenuScreen() {
    const [roomCode, setRoomCode] = React.useState("");
    const [newName, setName] = React.useState("");
    const [state, setState] = React.useState(MenuState.Join);

    const { data: profile, mutate: setProfile } = useSWR(
        "fetch-profile",
        commands.getProfile,
        sharedSwrConfig
    );
    const { data: gameHistory } = useSWR(
        "list-game-history",
        commands.listGameHistories,
        sharedSwrConfig
    );

    const onSaveProfile = async () => {
        await commands.updateProfile({ ...profile, display_name: newName });
        setProfile({ ...profile, display_name: newName });
    };

    return (
        <div className="h-screen v-screen flex flex-col items-center justify-center font-sans">
            <MenuRouter state={state}></MenuRouter>
            <NavBar state={state} setState={setState}></NavBar>
        </div>
    );
}
