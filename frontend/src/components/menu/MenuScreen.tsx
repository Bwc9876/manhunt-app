import { commands, GameSettings, PlayerProfile } from "@/bindings";
import React, { Dispatch, SetStateAction } from "react";
import NavButton from "./NavButton";
import JoinLobby from "./JoinLobby";
import {
    IoAddOutline,
    IoArrowForward,
    IoAccessibilityOutline,
    IoCalendarClearOutline
} from "react-icons/io5";
import EditProfile from "./EditProfile";
import useSWR, { KeyedMutator } from "swr";
import { sharedSwrConfig } from "@/lib/hooks";
import CreateGame from "./CreateGame";

// Temp settings for now.
const defaultSettings: GameSettings = {
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

export function MenuRouter({
    state,
    profile,
    setProfile,
    settings,
    setSettings
}: {
    state: MenuState;
    profile: PlayerProfile;
    setProfile: KeyedMutator<PlayerProfile>;
    settings: GameSettings;
    setSettings: React.Dispatch<React.SetStateAction<GameSettings>>;
}) {
    switch (state) {
        case MenuState.Join:
            return <JoinLobby settings={settings} />;

        case MenuState.Create:
            return <CreateGame settings={settings} setSettings={setSettings} />;

        case MenuState.Profile:
            return <EditProfile profile={profile} setProfile={setProfile} />;

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
        <div className="w-full h-1/8 flex flex-row border-t-1 justify-center items-center bg-white border-gray-300 fixed bottom-0">
            <NavButton current={state} setCurrent={setState} target={MenuState.Join}>
                <IoArrowForward className="size-5" />
                Join
            </NavButton>

            <NavButton current={state} setCurrent={setState} target={MenuState.Create}>
                <IoAddOutline className="size-5" />
                Create
            </NavButton>

            <NavButton current={state} setCurrent={setState} target={MenuState.Profile}>
                <IoAccessibilityOutline className="size-5" />
                Profile
            </NavButton>

            <NavButton current={state} setCurrent={setState} target={MenuState.History}>
                <IoCalendarClearOutline className="size-5" />
                History
            </NavButton>
        </div>
    );
}

export default function MenuScreen() {
    const [state, setState] = React.useState(MenuState.Join);
    const [settings, setSettings] = React.useState(defaultSettings);

    const { data: profile, mutate: setProfile } = useSWR(
        "fetch-profile",
        commands.getProfile,
        sharedSwrConfig
    );

    return (
        <div className="screen">
            <MenuRouter
                state={state}
                profile={profile}
                setProfile={setProfile}
                settings={settings}
                setSettings={setSettings}
            ></MenuRouter>

            <NavBar state={state} setState={setState}></NavBar>
        </div>
    );
}
