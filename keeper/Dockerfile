FROM node:22-slim AS build
WORKDIR /keeper
COPY package.json yarn.lock ./
RUN yarn install --frozen-lockfile
COPY tsconfig.json ./
COPY idl ./idl
COPY src ./src
RUN yarn build && yarn install --frozen-lockfile --production

FROM node:22-slim
WORKDIR /keeper
ENV NODE_ENV=production
COPY --from=build /keeper/node_modules ./node_modules
COPY --from=build /keeper/dist ./dist
COPY migrations ./migrations
COPY package.json ./
USER node
CMD ["node", "dist/src/index.js"]
