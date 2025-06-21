import { Result } from "@/bindings.ts";

export const unwrapResult = <T, E>(res: Result<T, E>): T => {
    switch (res.status) {
        case "ok":
            return res.data;
        case "error":
            throw res.error;
    }
};
